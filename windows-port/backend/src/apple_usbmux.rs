use serde::Serialize;

#[derive(Serialize)]
pub struct UsbMuxHandshakeResult {
    pub interface_path: String,
    pub configuration_value_expected: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub bulk_out_endpoint: String,
    pub bulk_in_endpoint: String,
    pub max_packet_size: u16,
    pub version_request_hex: String,
    pub version_reply_hex: String,
    pub setup_frame_hex: String,
    pub bytes_received: u32,
    pub trailing_bytes: u32,
    pub version: u32,
    pub setup_sent: bool,
    pub message: String,
}

#[cfg(windows)]
mod platform {
    use super::UsbMuxHandshakeResult;
    use std::{ffi::c_void, mem, ptr};
    use windows_sys::core::GUID;
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::{
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
            SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
            SP_DEVICE_INTERFACE_DATA, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        },
        Foundation::{
            CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE,
            INVALID_HANDLE_VALUE, ERROR_NO_MORE_ITEMS,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
    };

    type WinUsbHandle = *mut c_void;

    #[repr(C)]
    struct UsbInterfaceDescriptor {
        length: u8,
        descriptor_type: u8,
        interface_number: u8,
        alternate_setting: u8,
        endpoint_count: u8,
        interface_class: u8,
        interface_subclass: u8,
        interface_protocol: u8,
        interface_index: u8,
    }

    #[repr(C)]
    #[derive(Default)]
    struct WinUsbPipeInformation {
        pipe_type: u32,
        pipe_id: u8,
        maximum_packet_size: u16,
        interval: u8,
    }

    #[link(name = "winusb")]
    unsafe extern "system" {
        fn WinUsb_Initialize(device_handle: HANDLE, interface_handle: *mut WinUsbHandle) -> i32;
        fn WinUsb_Free(interface_handle: WinUsbHandle) -> i32;
        fn WinUsb_QueryInterfaceSettings(
            interface_handle: WinUsbHandle,
            alternate_setting: u8,
            descriptor: *mut UsbInterfaceDescriptor,
        ) -> i32;
        fn WinUsb_QueryPipe(
            interface_handle: WinUsbHandle,
            alternate_setting: u8,
            pipe_index: u8,
            pipe_info: *mut WinUsbPipeInformation,
        ) -> i32;
        fn WinUsb_SetPipePolicy(
            interface_handle: WinUsbHandle,
            pipe_id: u8,
            policy_type: u32,
            value_length: u32,
            value: *const c_void,
        ) -> i32;
        fn WinUsb_ReadPipe(
            interface_handle: WinUsbHandle,
            pipe_id: u8,
            buffer: *mut u8,
            buffer_length: u32,
            length_transferred: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WinUsb_WritePipe(
            interface_handle: WinUsbHandle,
            pipe_id: u8,
            buffer: *const u8,
            buffer_length: u32,
            length_transferred: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    const APPLE_MUX_GUID: GUID = GUID {
        data1: 0xf0b32be3,
        data2: 0x6678,
        data3: 0x4879,
        data4: [0x92, 0x30, 0xe4, 0x38, 0x45, 0xd8, 0x05, 0xee],
    };
    const USBMUX_INTERFACE_CLASS: u8 = 0xff;
    const USBMUX_INTERFACE_SUBCLASS: u8 = 0xfe;
    const USBMUX_INTERFACE_PROTOCOL: u8 = 0x02;
    const USBMUX_INTERFACE_NUMBER: u8 = 1;
    const BULK_PIPE_TYPE: u32 = 2;
    const PIPE_TRANSFER_TIMEOUT: u32 = 3;
    const IO_TIMEOUT_MS: u32 = 60_000;
    const MAX_FRAME_BYTES: usize = 65_536;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join("-")
    }

    fn read_mux_paths() -> Result<Vec<String>, String> {
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
            let found = unsafe {
                SetupDiEnumDeviceInterfaces(set, ptr::null(), &APPLE_MUX_GUID, index, &mut data)
            };
            if found == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_NO_MORE_ITEMS { break; }
                unsafe { SetupDiDestroyDeviceInfoList(set); }
                return Err(format!("SetupDiEnumDeviceInterfaces failed: {error}"));
            }
            index += 1;

            let mut required = 0u32;
            unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set, &data, ptr::null_mut(), 0, &mut required, ptr::null_mut(),
                );
            }
            if required < 6 { continue; }

            let mut detail = vec![0u8; required as usize];
            let cb_size: u32 = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
            detail[..4].copy_from_slice(&cb_size.to_ne_bytes());
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set, &data, detail.as_mut_ptr() as *mut _, required,
                    ptr::null_mut(), ptr::null_mut(),
                )
            };
            if ok == 0 { continue; }

            let words = unsafe {
                std::slice::from_raw_parts(
                    detail.as_ptr().add(4) as *const u16,
                    (required as usize - 4) / 2,
                )
            };
            let length = words.iter().position(|word| *word == 0).unwrap_or(words.len());
            let path = String::from_utf16_lossy(&words[..length]);
            if path.to_ascii_uppercase().contains("VID_05AC&PID_") {
                paths.push(path);
            }
        }

        unsafe { SetupDiDestroyDeviceInfoList(set); }
        if paths.is_empty() {
            return Err("Apple MUX1 interface path not found; reconnect the iPhone and check the Apple driver.".into());
        }
        Ok(paths)
    }

    fn open_mux() -> Result<(HANDLE, String), String> {
        let mut errors = Vec::new();
        for path in read_mux_paths()? {
            let path_wide = wide(&path);
            let handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(), GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null(),
                    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok((handle, path));
            }
            errors.push(format!("{path} -> Win32 {}", unsafe { GetLastError() }));
        }
        Err(format!("Apple MUX1 paths found, but CreateFileW failed: {}", errors.join("; ")))
    }

    fn query_bulk_pipes(
        interface: WinUsbHandle,
    ) -> Result<(u8, u8, u16, u8, u8), String> {
        let mut descriptor: UsbInterfaceDescriptor = unsafe { mem::zeroed() };
        if unsafe { WinUsb_QueryInterfaceSettings(interface, 0, &mut descriptor) } == 0 {
            return Err(format!("WinUsb_QueryInterfaceSettings(alt=0) failed: {}", unsafe { GetLastError() }));
        }

        if descriptor.interface_number != USBMUX_INTERFACE_NUMBER ||
            descriptor.interface_class != USBMUX_INTERFACE_CLASS ||
            descriptor.interface_subclass != USBMUX_INTERFACE_SUBCLASS ||
            descriptor.interface_protocol != USBMUX_INTERFACE_PROTOCOL {
            return Err(format!(
                "Unexpected interface descriptor: number={} class={:02X}/{:02X}/{:02X}",
                descriptor.interface_number, descriptor.interface_class,
                descriptor.interface_subclass, descriptor.interface_protocol,
            ));
        }

        let mut bulk_out = None;
        let mut bulk_in = None;
        let mut packet_size = 0u16;

        for pipe_index in 0..descriptor.endpoint_count {
            let mut pipe = WinUsbPipeInformation::default();
            if unsafe { WinUsb_QueryPipe(interface, 0, pipe_index, &mut pipe) } == 0 {
                return Err(format!("WinUsb_QueryPipe(index={pipe_index}) failed: {}", unsafe { GetLastError() }));
            }
            if pipe.pipe_type != BULK_PIPE_TYPE { continue; }
            if pipe.pipe_id == 0x04 {
                bulk_out = Some(pipe.pipe_id);
                packet_size = pipe.maximum_packet_size;
            } else if pipe.pipe_id == 0x85 {
                bulk_in = Some(pipe.pipe_id);
                packet_size = pipe.maximum_packet_size;
            }
        }

        match (bulk_out, bulk_in) {
            (Some(out_pipe), Some(in_pipe)) => Ok((
                out_pipe, in_pipe, packet_size,
                descriptor.interface_number, descriptor.alternate_setting,
            )),
            _ => Err(format!(
                "USBMUX bulk endpoints 0x04 OUT / 0x85 IN were not both exposed by WinUSB (endpoint_count={})",
                descriptor.endpoint_count,
            )),
        }
    }

    fn set_timeout(interface: WinUsbHandle, pipe: u8) -> Result<(), String> {
        let timeout = IO_TIMEOUT_MS;
        let ok = unsafe {
            WinUsb_SetPipePolicy(
                interface, pipe, PIPE_TRANSFER_TIMEOUT,
                mem::size_of::<u32>() as u32,
                &timeout as *const u32 as *const c_void,
            )
        };
        if ok == 0 {
            return Err(format!("WinUsb_SetPipePolicy(pipe=0x{pipe:02X}) failed: {}", unsafe { GetLastError() }));
        }
        Ok(())
    }

    fn read_frame(interface: WinUsbHandle, pipe: u8) -> Result<(Vec<u8>, u32), String> {
        let mut bytes = Vec::with_capacity(256);
        let mut buffer = vec![0u8; 4096];
        let mut total_read = 0u32;

        loop {
            if bytes.len() >= 16 {
                let protocol = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
                let length = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
                if !(16..=MAX_FRAME_BYTES).contains(&length) {
                    return Err(format!("Invalid USBMUX frame length {length}; header={}", hex(&bytes[..16])));
                }
                if bytes.len() >= length {
                    return Ok((bytes, total_read));
                }
                if protocol != 0 {
                    return Err(format!(
                        "Expected USBMUX version protocol 0, received protocol {protocol}; header={}",
                        hex(&bytes[..16]),
                    ));
                }
            }

            let mut transferred = 0u32;
            let ok = unsafe {
                WinUsb_ReadPipe(
                    interface, pipe, buffer.as_mut_ptr(), buffer.len() as u32,
                    &mut transferred, ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(format!(
                    "WinUsb_ReadPipe(pipe=0x{pipe:02X}) failed: Win32 {} (received {} bytes so far)",
                    unsafe { GetLastError() }, bytes.len(),
                ));
            }
            if transferred == 0 {
                return Err("WinUsb_ReadPipe completed with zero bytes before the USBMUX version reply".into());
            }
            total_read = total_read.saturating_add(transferred);
            if bytes.len().saturating_add(transferred as usize) > MAX_FRAME_BYTES + 64 {
                return Err("USBMUX version reply exceeded the bounded receive buffer".into());
            }
            bytes.extend_from_slice(&buffer[..transferred as usize]);
        }
    }

    pub fn handshake() -> Result<UsbMuxHandshakeResult, String> {
        let (device, path) = open_mux()?;
        let mut interface: WinUsbHandle = ptr::null_mut();

        let result = (|| {
            if unsafe { WinUsb_Initialize(device, &mut interface) } == 0 {
                return Err(format!("WinUsb_Initialize(MUX1) failed: {}", unsafe { GetLastError() }));
            }

            let (out_pipe, in_pipe, packet_size, interface_number, alternate_setting) =
                query_bulk_pipes(interface)?;
            set_timeout(interface, out_pipe)?;
            set_timeout(interface, in_pipe)?;

            // USBMUX v2 version request: protocol=0, length=20, version=2, reserved=0.
            let mut version_request = [0u8; 20];
            version_request[4..8].copy_from_slice(&20u32.to_be_bytes());
            version_request[8..12].copy_from_slice(&2u32.to_be_bytes());

            let mut written = 0u32;
            let write_ok = unsafe {
                WinUsb_WritePipe(
                    interface, out_pipe, version_request.as_ptr() as *mut u8,
                    version_request.len() as u32, &mut written, ptr::null_mut(),
                )
            };
            if write_ok == 0 {
                return Err(format!("USBMUX v2 version request write failed: Win32 {}", unsafe { GetLastError() }));
            }
            if written != version_request.len() as u32 {
                return Err(format!("USBMUX version request short write: {written}/{}", version_request.len()));
            }

            let (reply_wire, bytes_received) = read_frame(interface, in_pipe)?;
            if reply_wire.len() < 20 {
                return Err(format!("USBMUX version reply too short: {} bytes", reply_wire.len()));
            }
            let protocol = u32::from_be_bytes(reply_wire[0..4].try_into().unwrap());
            let frame_length = u32::from_be_bytes(reply_wire[4..8].try_into().unwrap());
            let version = u32::from_be_bytes(reply_wire[8..12].try_into().unwrap());
            if protocol != 0 || frame_length != 20 || version != 2 {
                return Err(format!(
                    "USBMUX v2 version reply mismatch: protocol={protocol} length={frame_length} version={version} reply={}",
                    hex(&reply_wire[..reply_wire.len().min(64)]),
                ));
            }

            // The fourth header word of the version reply carries sequence/ack fields,
            // not the request magic. Mirror the Android host's first SETUP frame.
            let peer_sequence = u16::from_be_bytes([reply_wire[12], reply_wire[13]]);
            let mut setup = [0u8; 17];
            setup[0..4].copy_from_slice(&2u32.to_be_bytes());
            setup[4..8].copy_from_slice(&17u32.to_be_bytes());
            setup[8..12].copy_from_slice(&0xfeedfaceu32.to_be_bytes());
            setup[12..14].copy_from_slice(&0u16.to_be_bytes());
            setup[14..16].copy_from_slice(&peer_sequence.to_be_bytes());
            setup[16] = 0x07;

            let mut setup_written = 0u32;
            let setup_ok = unsafe {
                WinUsb_WritePipe(
                    interface, out_pipe, setup.as_ptr() as *mut u8,
                    setup.len() as u32, &mut setup_written, ptr::null_mut(),
                )
            };
            if setup_ok == 0 {
                return Err(format!("USBMUX v2 SETUP frame write failed: Win32 {}", unsafe { GetLastError() }));
            }
            if setup_written != setup.len() as u32 {
                return Err(format!("USBMUX SETUP short write: {setup_written}/{}", setup.len()));
            }

            let trailing = reply_wire.len().saturating_sub(frame_length as usize) as u32;
            Ok(UsbMuxHandshakeResult {
                interface_path: path.clone(),
                configuration_value_expected: 5,
                interface_number,
                alternate_setting,
                bulk_out_endpoint: format!("0x{out_pipe:02X}"),
                bulk_in_endpoint: format!("0x{in_pipe:02X}"),
                max_packet_size: packet_size,
                version_request_hex: hex(&version_request),
                version_reply_hex: hex(&reply_wire[..frame_length as usize]),
                setup_frame_hex: hex(&setup),
                bytes_received,
                trailing_bytes: trailing,
                version,
                setup_sent: true,
                message: "USBMUX v2 version exchange and SETUP frame completed. The bulk pipe is closed after this probe; TCP multiplexing and the persistent iAP2 stream are not started yet.".into(),
            })
        })();

        if !interface.is_null() {
            unsafe { WinUsb_Free(interface); }
        }
        unsafe { CloseHandle(device); }
        result
    }
}

#[cfg(windows)]
pub fn handshake() -> Result<UsbMuxHandshakeResult, String> {
    platform::handshake()
}

#[cfg(not(windows))]
pub fn handshake() -> Result<UsbMuxHandshakeResult, String> {
    Err("Apple USBMUX bulk transfers are supported on Windows only.".into())
}
