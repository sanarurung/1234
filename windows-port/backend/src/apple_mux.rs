use serde::Serialize;

#[derive(Serialize)]
pub struct AppleMuxDescriptorResult {
    pub interface_path: String,
    pub vendor_id: String,
    pub product_id: String,
    pub descriptor_hex: String,
    pub response_hex: String,
    pub bytes_returned: u32,
}

#[derive(Serialize)]
pub struct AppleMuxModeRequestResult {
    pub interface_path: String,
    pub vendor_id: String,
    pub product_id: String,
    pub request: String,
    pub response_hex: String,
    pub bytes_returned: u32,
    pub reenumeration_expected: bool,
    pub message: String,
}


#[derive(Serialize)]
pub struct AppleMuxModeDiagnosticResult {
    pub interface_path: String,
    pub active_configuration: Option<u8>,
    pub usb_mode_read: usize,
    pub usb_mode_hex: String,
    pub usb_mode_values: Vec<u8>,
    pub message: String,
}

#[cfg(windows)]
mod platform {
    use super::{AppleMuxDescriptorResult, AppleMuxModeRequestResult};
    use std::{ffi::c_void, mem, ptr};
    use windows_sys::core::GUID;
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::{
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
            SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
            SP_DEVICE_INTERFACE_DATA, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        },
        Foundation::{CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, ERROR_NO_MORE_ITEMS},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    };

    const APPLE_MUX_GUID: GUID = GUID {
        data1: 0xf0b32be3,
        data2: 0x6678,
        data3: 0x4879,
        data4: [0x92, 0x30, 0xe4, 0x38, 0x45, 0xd8, 0x05, 0xee],
    };
    const APPLE_CONTROL_IOCTL: u32 = 0x2200A0;
    const GET_DESCRIPTOR: u8 = 0x06;
    const GET_DEVICE_DESCRIPTOR: u16 = 0x0100;
    const CARPLAY_CONFIGURATION_REQUEST: u8 = 0x52;
    const CARPLAY_CONFIGURATION_INDEX: u16 = 0x0004;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join("-")
    }

    fn enumerate_mux_paths() -> Result<Vec<String>, String> {
        let set = unsafe {
            SetupDiGetClassDevsW(
                &APPLE_MUX_GUID, ptr::null(), ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        };
        if set == -1isize {
            return Err(format!("SetupDiGetClassDevsW(Apple MUX1) failed: {}", unsafe { GetLastError() }));
        }
        let mut paths = Vec::new();
        let mut index = 0u32;
        loop {
            let mut data: SP_DEVICE_INTERFACE_DATA = unsafe { mem::zeroed() };
            data.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            let found = unsafe { SetupDiEnumDeviceInterfaces(set, ptr::null(), &APPLE_MUX_GUID, index, &mut data) };
            if found == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_NO_MORE_ITEMS { break; }
                unsafe { SetupDiDestroyDeviceInfoList(set); }
                return Err(format!("SetupDiEnumDeviceInterfaces failed: {error}"));
            }
            index += 1;
            let mut required = 0u32;
            unsafe { SetupDiGetDeviceInterfaceDetailW(set, &data, ptr::null_mut(), 0, &mut required, ptr::null_mut()); }
            if required < 6 { continue; }
            let mut detail = vec![0u8; required as usize];
            let cb_size: u32 = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
            detail[..4].copy_from_slice(&cb_size.to_ne_bytes());
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(set, &data, detail.as_mut_ptr() as *mut _, required, ptr::null_mut(), ptr::null_mut())
            };
            if ok == 0 { continue; }
            let words = unsafe {
                std::slice::from_raw_parts(detail.as_ptr().add(4) as *const u16, (required as usize - 4) / 2)
            };
            let length = words.iter().position(|word| *word == 0).unwrap_or(words.len());
            let path = String::from_utf16_lossy(&words[..length]);
            if path.to_ascii_uppercase().contains("VID_05AC&PID_") { paths.push(path); }
        }
        unsafe { SetupDiDestroyDeviceInfoList(set); }
        if paths.is_empty() {
            return Err("Apple MUX1 interface path not found; check iPhone connection and Apple driver.".into());
        }
        Ok(paths)
    }

    fn open_apple_mux() -> Result<(HANDLE, String), String> {
        let mut errors = Vec::new();
        for path in enumerate_mux_paths()? {
            let path_wide = wide(&path);
            let handle = unsafe {
                CreateFileW(path_wide.as_ptr(), GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null(), OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL, ptr::null_mut())
            };
            if handle != INVALID_HANDLE_VALUE { return Ok((handle, path)); }
            errors.push(format!("{} -> Win32 {}", path, unsafe { GetLastError() }));
        }
        Err(format!("Apple MUX1 paths found, but open failed: {}", errors.join("; ")))
    }

    fn control_in(handle: HANDLE, request_type: u8, request: u8, value: u16, index: u16, length: u16)
        -> Result<(Vec<u8>, u32), String>
    {
        let mut packet = vec![0u8; 8 + usize::from(length)];
        packet[0] = request_type;
        packet[1] = request;
        packet[2..4].copy_from_slice(&value.to_le_bytes());
        packet[4..6].copy_from_slice(&index.to_le_bytes());
        packet[6..8].copy_from_slice(&length.to_le_bytes());
        let mut output = vec![0u8; packet.len()];
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(handle, APPLE_CONTROL_IOCTL,
                packet.as_ptr() as *const c_void, packet.len() as u32,
                output.as_mut_ptr() as *mut c_void, output.len() as u32,
                &mut returned, ptr::null_mut())
        };
        if ok == 0 {
            return Err(format!("Apple MUX1 CONTROL IOCTL failed (Win32 {})", unsafe { GetLastError() }));
        }
        if returned < 8 || returned as usize > output.len() {
            return Err(format!("Invalid Apple MUX1 control response length: {returned}"));
        }
        output.truncate(returned as usize);
        Ok((output, returned))
    }

    fn verify_device_descriptor(handle: HANDLE) -> Result<(Vec<u8>, u32, u32, u32), String> {
        let (response, returned) = control_in(handle, 0x80, GET_DESCRIPTOR, GET_DEVICE_DESCRIPTOR, 0, 18)?;
        if response.len() != 26 || response[8..10] != [18, 1] {
            return Err(format!("Invalid USB Device Descriptor response: {}", hex(&response)));
        }
        let vid = u16::from_le_bytes([response[16], response[17]]);
        let pid = u16::from_le_bytes([response[18], response[19]]);
        if vid != 0x05AC { return Err(format!("Not an Apple device (VID={vid:04X})")); }
        Ok((response[8..].to_vec(), u32::from(vid), u32::from(pid), returned))
    }


    pub fn read_mode_diagnostic() -> Result<AppleMuxModeDiagnosticResult, String> {
        let (handle, path) = open_apple_mux()?;
        let result = (|| {
            let (config, config_n) = control_in(handle, 0x80, 0x08, 0, 0, 1)?;
            let active_configuration = if config_n >= 9 { Some(config[8]) } else { None };
            let (mode, mode_n) = control_in(handle, 0xC0, 0x45, 0, 0, 4)?;
            let values = if mode_n >= 8 { mode[8..mode_n as usize].to_vec() } else { Vec::new() };
            Ok(AppleMuxModeDiagnosticResult {
                interface_path: path,
                active_configuration,
                usb_mode_read: values.len(),
                usb_mode_hex: hex(&values),
                usb_mode_values: values,
                message: format!("Read-only USB state. GET_CONFIGURATION bytes={config_n}; GET_USB_MODE bytes={mode_n}. No configuration or driver state was changed."),
            })
        })();
        unsafe { CloseHandle(handle); }
        result
    }

    pub fn read_descriptor() -> Result<AppleMuxDescriptorResult, String> {
        let (handle, path) = open_apple_mux()?;
        let result = (|| {
            let (descriptor, vid, pid, returned) = verify_device_descriptor(handle)?;
            let response = {
                let mut packet = vec![0u8; 26];
                packet[..8].copy_from_slice(&[0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00]);
                // Descriptor bytes were already validated; return the exact validated bytes with setup header.
                packet[8..].copy_from_slice(&descriptor);
                packet
            };
            Ok(AppleMuxDescriptorResult {
                interface_path: path, vendor_id: format!("{vid:04X}"), product_id: format!("{pid:04X}"),
                descriptor_hex: hex(&descriptor), response_hex: hex(&response), bytes_returned: returned,
            })
        })();
        unsafe { CloseHandle(handle); }
        result
    }

    pub fn request_carplay_mode() -> Result<AppleMuxModeRequestResult, String> {
        let (handle, path) = open_apple_mux()?;
        let result = (|| {
            let (_, vid, pid, _) = verify_device_descriptor(handle)?;
            let (response, returned) = control_in(handle, 0xC0, CARPLAY_CONFIGURATION_REQUEST, 0, CARPLAY_CONFIGURATION_INDEX, 1)?;
            if response.len() != 9 {
                return Err(format!("CarPlay mode request returned {} data bytes; expected 1 ({})",
                    response.len().saturating_sub(8), hex(&response)));
            }
            Ok(AppleMuxModeRequestResult {
                interface_path: path, vendor_id: format!("{vid:04X}"), product_id: format!("{pid:04X}"),
                request: "bmRequestType=0xC0, bRequest=0x52, wValue=0, wIndex=4, wLength=1".into(),
                response_hex: hex(&response[8..]), bytes_returned: returned,
                reenumeration_expected: true,
                message: "Vendor request completed. The iPhone may disconnect and re-enumerate; wait for the new USB device before opening the iAP2 USBMUX interface.".into(),
            })
        })();
        unsafe { CloseHandle(handle); }
        result
    }
}

#[cfg(windows)]
pub fn read_descriptor() -> Result<AppleMuxDescriptorResult, String> { platform::read_descriptor() }
#[cfg(not(windows))]
pub fn read_descriptor() -> Result<AppleMuxDescriptorResult, String> {
    Err("Apple MUX1 control transfers are supported on Windows only.".into())
}
#[cfg(windows)]
pub fn request_carplay_mode() -> Result<AppleMuxModeRequestResult, String> { platform::request_carplay_mode() }
#[cfg(not(windows))]
pub fn request_carplay_mode() -> Result<AppleMuxModeRequestResult, String> {
    Err("Apple MUX1 control transfers are supported on Windows only.".into())
}

#[cfg(windows)]
pub fn read_mode_diagnostic() -> Result<AppleMuxModeDiagnosticResult, String> { platform::read_mode_diagnostic() }
#[cfg(not(windows))]
pub fn read_mode_diagnostic() -> Result<AppleMuxModeDiagnosticResult, String> { Err("Apple MUX diagnostics are supported on Windows only.".into()) }
