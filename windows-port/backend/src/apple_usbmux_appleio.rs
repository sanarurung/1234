use serde::Serialize;

#[derive(Serialize)]
pub struct AppleIoUsbMuxResult {
    pub interface_path: String,
    pub device_serial: String,
    pub pipe_1_endpoint: String,
    pub pipe_1_max_packet: u16,
    pub pipe_2_endpoint: String,
    pub pipe_2_max_packet: u16,
    pub read_pipe_index: u8,
    pub write_pipe_index: u8,
    pub version_request_hex: String,
    pub version_reply_hex: String,
    pub version: u32,
    pub message: String,
}

#[cfg(windows)]
mod platform {
    use super::AppleIoUsbMuxResult;
    use std::{ffi::c_void, io, mem, ptr};
    use windows_sys::core::GUID;
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::{
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
            SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
            SP_DEVICE_INTERFACE_DATA, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        },
        Foundation::{
            CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE,
            INVALID_HANDLE_VALUE, ERROR_IO_PENDING, ERROR_NO_MORE_ITEMS,
        },
        Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    };

    const APPLE_FILTER_GUID: GUID = GUID {
        data1: 0x664be590,
        data2: 0x54bd,
        data3: 0x4964,
        data4: [0x8a, 0x8c, 0x6c, 0xd1, 0x31, 0x4f, 0x6d, 0xc2],
    };

    const IOCTL_READ_PIPE_BASE: u32 = 0x220000;
    const IOCTL_WRITE_PIPE_BASE: u32 = 0x220000;
    const IOCTL_INIT: u32 = 0x2200A8;
    const IOCTL_GET_PIPE_PROPERTIES: u32 = 0x2200A4;
    const IOCTL_GET_SERIAL: u32 = 0x2200AC;
    const USBMUX_VERSION: u32 = 0;
    const USBMUX_VERSION_REQUEST_SIZE: usize = 20;
    const MAX_REPLY: usize = 0x8000;
    const IO_TIMEOUT_MS: u32 = 15000;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: HANDLE,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            handle: HANDLE,
            code: u32,
            input: *const c_void,
            input_len: u32,
            output: *mut c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn CreateEventW(attributes: *const c_void, manual_reset: i32, initial_state: i32, name: *const u16) -> HANDLE;
        fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
        fn GetOverlappedResult(handle: HANDLE, overlapped: *mut Overlapped, transferred: *mut u32, wait: i32) -> i32;
        fn CancelIoEx(handle: HANDLE, overlapped: *mut Overlapped) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("-")
    }

    fn enumerate_paths() -> Result<Vec<String>, String> {
        let set = unsafe {
            SetupDiGetClassDevsW(&APPLE_FILTER_GUID, ptr::null(), ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
        };
        if set == -1isize {
            return Err(format!("SetupDiGetClassDevsW(AppleUsbFilter GUID) failed: {}", unsafe { GetLastError() }));
        }
        let mut paths = Vec::new();
        let mut index = 0u32;
        loop {
            let mut iface: SP_DEVICE_INTERFACE_DATA = unsafe { mem::zeroed() };
            iface.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            let ok = unsafe { SetupDiEnumDeviceInterfaces(set, ptr::null(), &APPLE_FILTER_GUID, index, &mut iface) };
            if ok == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_NO_MORE_ITEMS { break; }
                unsafe { SetupDiDestroyDeviceInfoList(set); }
                return Err(format!("SetupDiEnumDeviceInterfaces failed: {error}"));
            }
            index += 1;
            let mut required = 0u32;
            unsafe { SetupDiGetDeviceInterfaceDetailW(set, &iface, ptr::null_mut(), 0, &mut required, ptr::null_mut()); }
            if required < 6 { continue; }
            let mut detail = vec![0u8; required as usize];
            let cb_size: u32 = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
            detail[..4].copy_from_slice(&cb_size.to_ne_bytes());
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(set, &iface, detail.as_mut_ptr() as *mut _, required, ptr::null_mut(), ptr::null_mut())
            };
            if ok == 0 { continue; }
            let words = unsafe {
                std::slice::from_raw_parts(detail.as_ptr().add(4) as *const u16, (required as usize - 4) / 2)
            };
            let length = words.iter().position(|w| *w == 0).unwrap_or(words.len());
            let base = String::from_utf16_lossy(&words[..length]);
            if base.to_ascii_uppercase().contains("VID_05AC&PID_") {
                // AppleUsbFilter registers the base interface. The gate endpoint is opened via this reference suffix.
                paths.push(format!("{base}\\MUX1"));
            }
        }
        unsafe { SetupDiDestroyDeviceInfoList(set); }
        if paths.is_empty() { return Err("AppleUsbFilter interface found no iPhone MUX1 path (VID_05AC).".into()); }
        Ok(paths)
    }

    unsafe fn ioctl(
        handle: HANDLE, code: u32, input: *const c_void, input_len: u32,
        output: *mut c_void, output_len: u32,
    ) -> io::Result<u32> {
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() || event == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut ov = Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event };
        let mut transferred = 0u32;
        let ok = unsafe { DeviceIoControl(handle, code, input, input_len, output, output_len, &mut transferred, &mut ov) };
        let result = if ok != 0 {
            Ok(transferred)
        } else {
            let error = unsafe { GetLastError() };
            if error != ERROR_IO_PENDING {
                Err(io::Error::from_raw_os_error(error as i32))
            } else {
                match unsafe { WaitForSingleObject(event, IO_TIMEOUT_MS) } {
                    WAIT_OBJECT_0 => {
                        if unsafe { GetOverlappedResult(handle, &mut ov, &mut transferred, 0) } != 0 {
                            Ok(transferred)
                        } else {
                            Err(io::Error::last_os_error())
                        }
                    }
                    WAIT_TIMEOUT => {
                        unsafe { CancelIoEx(handle, &mut ov); }
                        Err(io::Error::new(io::ErrorKind::TimedOut, format!("Apple MUX IOCTL 0x{code:08X} timed out after {IO_TIMEOUT_MS} ms")))
                    }
                    other => Err(io::Error::other(format!("WaitForSingleObject returned {other}"))),
                }
            }
        };
        unsafe { CloseHandle(event); }
        result
    }

    fn pipe_ioctl_read(pipe: u8) -> u32 {
        IOCTL_READ_PIPE_BASE | (((pipe as u32) << 2) + 0x20) | 2
    }

    fn pipe_ioctl_write(pipe: u8) -> u32 {
        IOCTL_WRITE_PIPE_BASE | (((pipe as u32) << 2) + 0x40) | 1
    }

    fn pipe_properties(handle: HANDLE, index: u32) -> Result<(u8, u16), String> {
        let mut buffer = [0u8; 0x18];
        buffer[..4].copy_from_slice(&index.to_le_bytes());
        let n = unsafe {
            ioctl(handle, IOCTL_GET_PIPE_PROPERTIES, buffer.as_ptr() as *const c_void, 0x18, buffer.as_mut_ptr() as *mut c_void, 0x18)
        }.map_err(|e| format!("Apple MUX GET_PIPE_PROPERTIES(index={index}) failed: {e}"))?;
        if n < 7 { return Err(format!("GET_PIPE_PROPERTIES returned only {n} bytes for pipe {index}")); }
        let packet = u16::from_le_bytes([buffer[4], buffer[5]]);
        Ok((buffer[6], packet))
    }

    pub fn handshake() -> Result<AppleIoUsbMuxResult, String> {
        let mut last_errors = Vec::new();
        for path in enumerate_paths()? {
            let path_w = wide(&path);
            let handle = unsafe {
                CreateFileW(path_w.as_ptr(), GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null(), OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED, ptr::null_mut())
            };
            if handle == INVALID_HANDLE_VALUE {
                last_errors.push(format!("{path}: CreateFileW Win32 {}", unsafe { GetLastError() }));
                continue;
            }

            let result = (|| {
                // Apple requires this 0x24-byte initialization IOCTL before any pipe operation.
                let init = [0u8; 0x24];
                unsafe {
                    ioctl(handle, IOCTL_INIT, init.as_ptr() as *const c_void, init.len() as u32,
                        init.as_ptr() as *mut c_void, init.len() as u32)
                }.map_err(|e| format!("Apple MUX INIT (0x2200A8) failed: {e}"))?;

                let mut serial_buf = [0u8; 0x100];
                let serial_n = unsafe {
                    ioctl(handle, IOCTL_GET_SERIAL, serial_buf.as_ptr() as *const c_void, 0x100,
                        serial_buf.as_mut_ptr() as *mut c_void, 0x100)
                }.map_err(|e| format!("Apple MUX GET_SERIAL (0x2200AC) failed: {e}"))? as usize;
                let serial_n = serial_n.min(serial_buf.len());
                let serial_end = serial_buf[..serial_n].iter().position(|b| *b == 0).unwrap_or(serial_n);
                let serial = String::from_utf8_lossy(&serial_buf[..serial_end]).into_owned();
                if serial.is_empty() { return Err("Apple MUX returned an empty UDID".into()); }

                let (ep1, mp1) = pipe_properties(handle, 1)?;
                let (ep2, mp2) = pipe_properties(handle, 2)?;
                let (read_pipe, write_pipe, write_packet) = match (ep1 & 0x80 != 0, ep2 & 0x80 != 0) {
                    (true, false) => (1u8, 2u8, mp2),
                    (false, true) => (2u8, 1u8, mp1),
                    _ => return Err(format!("Expected one bulk IN and one bulk OUT pipe; got pipe1=0x{ep1:02X}, pipe2=0x{ep2:02X}")),
                };

                let mut version_request = [0u8; USBMUX_VERSION_REQUEST_SIZE];
                version_request[0..4].copy_from_slice(&USBMUX_VERSION.to_be_bytes());
                version_request[4..8].copy_from_slice(&(USBMUX_VERSION_REQUEST_SIZE as u32).to_be_bytes());
                version_request[8..12].copy_from_slice(&2u32.to_be_bytes());

                let write_code = pipe_ioctl_write(write_pipe);
                // AppleUsbFilter's METHOD_IN_DIRECT pipe IOCTL expects the transfer
                // bytes in the output/MDL buffer too. netmuxd passes the same buffer
                // for input and output; a separate zeroed output buffer silently
                // sends zero bytes instead of the USBMUX frame.
                let mut write_buffer = version_request;
                let write_len = write_buffer.len() as u32;
                let write_ptr = write_buffer.as_mut_ptr();
                let written = unsafe {
                    ioctl(handle, write_code, write_ptr as *const c_void, write_len,
                        write_ptr as *mut c_void, write_len)
                }.map_err(|e| format!("Apple MUX bulk write (pipe {write_pipe}, IOCTL 0x{write_code:08X}) failed: {e}"))?;
                if written != write_len {
                    return Err(format!("USBMUX VERSION request short write: {written}/{write_len}"));
                }

                let read_code = pipe_ioctl_read(read_pipe);
                let mut reply = vec![0u8; MAX_REPLY];
                let read = unsafe {
                    ioctl(handle, read_code, reply.as_ptr() as *const c_void, reply.len() as u32,
                        reply.as_mut_ptr() as *mut c_void, reply.len() as u32)
                }.map_err(|e| format!("Apple MUX bulk read (pipe {read_pipe}, IOCTL 0x{read_code:08X}) failed: {e}"))? as usize;
                if read < 20 { return Err(format!("USBMUX VERSION reply too short: {read} bytes ({})", hex(&reply[..read]))); }
                reply.truncate(read);
                let protocol = u32::from_be_bytes(reply[0..4].try_into().unwrap());
                let frame_len = u32::from_be_bytes(reply[4..8].try_into().unwrap()) as usize;
                let version = u32::from_be_bytes(reply[8..12].try_into().unwrap());
                if protocol != 0 || frame_len < 20 || frame_len > reply.len() || version != 2 {
                    return Err(format!("Unexpected USBMUX VERSION reply: protocol={protocol}, length={frame_len}, version={version}, data={}", hex(&reply[..reply.len().min(64)])));
                }

                Ok(AppleIoUsbMuxResult {
                    interface_path: path.clone(),
                    device_serial: serial,
                    pipe_1_endpoint: format!("0x{ep1:02X}"),
                    pipe_1_max_packet: mp1,
                    pipe_2_endpoint: format!("0x{ep2:02X}"),
                    pipe_2_max_packet: mp2,
                    read_pipe_index: read_pipe,
                    write_pipe_index: write_pipe,
                    version_request_hex: hex(&version_request),
                    version_reply_hex: hex(&reply[..frame_len]),
                    version,
                    message: format!("AppleUsbFilter IOCTL transport is active. USBMUX VERSION v{version} exchange succeeded. No SETUP frame or persistent stream was started. OUT max packet: {write_packet} bytes."),
                })
            })();
            unsafe { CloseHandle(handle); }
            match result {
                Ok(value) => return Ok(value),
                Err(error) => last_errors.push(format!("{path}: {error}")),
            }
        }
        Err(format!("AppleUsbFilter USBMUX handshake failed: {}", last_errors.join(" | ")))
    }
}

#[cfg(windows)]
pub fn handshake() -> Result<AppleIoUsbMuxResult, String> { platform::handshake() }

#[cfg(not(windows))]
pub fn handshake() -> Result<AppleIoUsbMuxResult, String> {
    Err("AppleUsbFilter USBMUX transport is supported on Windows only.".into())
}
