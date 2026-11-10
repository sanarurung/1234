use serde::Serialize;
use std::io::{self, BufRead};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW, SP_DEVINFO_DATA, DIGCF_ALLCLASSES, DIGCF_PRESENT,
};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NO_MORE_ITEMS};

#[derive(Serialize)]
struct Device {
    name: String,
    hardware_ids: Vec<String>,
    vendor_id: Option<String>,
    product_id: Option<String>,
    interface_number: Option<String>,
}

#[derive(Serialize)]
struct EndpointInfo {
    index: u8,
    address: String,
    direction: &'static str,
    transfer_type: &'static str,
    max_packet_size: u16,
    interval: u8,
}

#[derive(Serialize)]
struct InterfaceInspection {
    interface_number: u8,
    alternate_setting: u8,
    endpoint_count: u8,
    interface_class: String,
    interface_subclass: String,
    interface_protocol: String,
    endpoints: Vec<EndpointInfo>,
}

#[derive(Serialize)]
struct InspectionResult {
    interface_path_found: bool,
    interfaces: Vec<InterfaceInspection>,
}

#[derive(Serialize)]
struct Response<T: Serialize> {
    event: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    devices: Option<T>,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn read_property(
    set: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    info: &SP_DEVINFO_DATA,
    property: u32,
) -> Vec<String> {
    let mut data_type = 0u32;
    let mut required = 0u32;
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(set, info, property, &mut data_type, std::ptr::null_mut(), 0, &mut required);
        if required == 0 { return Vec::new(); }
        let mut buffer = vec![0u8; required as usize + 2];
        if SetupDiGetDeviceRegistryPropertyW(set, info, property, &mut data_type, buffer.as_mut_ptr(), required, &mut required) == 0 {
            return Vec::new();
        }
        let words = std::slice::from_raw_parts(buffer.as_ptr() as *const u16, buffer.len() / 2);
        words.split(|word| *word == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf16_lossy(part))
            .collect()
    }
}

fn parse_id(ids: &[String], key: &str) -> Option<String> {
    for id in ids {
        let upper = id.to_ascii_uppercase();
        if let Some(index) = upper.find(key) {
            let value = upper.get(index + key.len()..index + key.len() + 4)?;
            if value.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(windows)]
fn enumerate_usb() -> Result<Vec<Device>, String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID,
    };
    let enumerator = wide("USB");
    let set = unsafe {
        SetupDiGetClassDevsW(
            std::ptr::null(),
            enumerator.as_ptr(),
            std::ptr::null_mut(),
            DIGCF_ALLCLASSES | DIGCF_PRESENT,
        )
    };

    if set == -1isize {
        return Err(format!("SetupDiGetClassDevsW failed: {}", unsafe { GetLastError() }));
    }

    let mut devices = Vec::new();
    let mut index = 0;
    loop {
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        let ok = unsafe { SetupDiEnumDeviceInfo(set, index, &mut info) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NO_MORE_ITEMS {
                unsafe { SetupDiDestroyDeviceInfoList(set); }
                return Err(format!("SetupDiEnumDeviceInfo failed: {error}"));
            }
            break;
        }
        index += 1;

        let ids = read_property(set, &info, SPDRP_HARDWAREID);
        let vendor_id = parse_id(&ids, "VID_");
        let product_id = parse_id(&ids, "PID_");
        if vendor_id.is_none() || product_id.is_none() { continue; }

        let friendly = read_property(set, &info, SPDRP_FRIENDLYNAME);
        let description = read_property(set, &info, SPDRP_DEVICEDESC);
        let name = friendly.first().or(description.first()).cloned()
            .unwrap_or_else(|| format!("USB Device {vendor_id:?}:{product_id:?}"));
        let interface_number = parse_id(&ids, "MI_");
        devices.push(Device { name, hardware_ids: ids, vendor_id, product_id, interface_number });
    }

    unsafe { SetupDiDestroyDeviceInfoList(set); }
    Ok(devices)
}

#[cfg(not(windows))]
fn enumerate_usb() -> Result<Vec<Device>, String> {
    Err("USB enumeration is currently supported on Windows only.".to_string())
}

#[cfg(windows)]
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

#[cfg(windows)]
#[repr(C)]
struct WinUsbPipeInformation {
    pipe_type: u32,
    pipe_id: u8,
    maximum_packet_size: u16,
    interval: u8,
}

#[cfg(windows)]
type WinUsbHandle = *mut std::ffi::c_void;

#[cfg(windows)]
#[link(name = "winusb")]
unsafe extern "system" {
    fn WinUsb_Initialize(device_handle: windows_sys::Win32::Foundation::HANDLE, interface_handle: *mut WinUsbHandle) -> i32;
    fn WinUsb_Free(interface_handle: WinUsbHandle) -> i32;
    fn WinUsb_QueryInterfaceSettings(interface_handle: WinUsbHandle, alternate_setting: u8, descriptor: *mut UsbInterfaceDescriptor) -> i32;
    fn WinUsb_QueryPipe(interface_handle: WinUsbHandle, alternate_setting: u8, pipe_index: u8, pipe_info: *mut WinUsbPipeInformation) -> i32;
}

#[cfg(windows)]
fn inspect_winusb_interfaces() -> Result<InspectionResult, String> {
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        SP_DEVICE_INTERFACE_DATA, DIGCF_DEVICEINTERFACE,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    // DeviceInterfaceGUIDs from the installed AppleUsbMux INF (oem19.inf).
    let interface_guid = GUID {
        data1: 0x664be590,
        data2: 0x54bd,
        data3: 0x4964,
        data4: [0x8a, 0x8c, 0x6c, 0xd1, 0x31, 0x4f, 0x6d, 0xc2],
    };

    let set = unsafe {
        SetupDiGetClassDevsW(&interface_guid, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
    };
    if set == -1isize {
        return Err(format!("SetupDiGetClassDevsW(interface GUID) failed: {}", unsafe { GetLastError() }));
    }

    let mut inspections = Vec::new();
    let mut diagnostics = Vec::new();
    let mut interface_paths_found = 0u32;
    let mut index = 0u32;
    loop {
        let mut interface_data: SP_DEVICE_INTERFACE_DATA = unsafe { std::mem::zeroed() };
        interface_data.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        let found = unsafe { SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &interface_guid, index, &mut interface_data) };
        if found == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_ITEMS { break; }
            unsafe { SetupDiDestroyDeviceInfoList(set); }
            return Err(format!("SetupDiEnumDeviceInterfaces failed: {error}"));
        }
        index += 1;

        let mut required = 0u32;
        unsafe { SetupDiGetDeviceInterfaceDetailW(set, &interface_data, std::ptr::null_mut(), 0, &mut required, std::ptr::null_mut()); }
        if required < 6 {
            diagnostics.push(format!("Interface #{index}: invalid detail buffer size ({required})"));
            continue;
        }
        let mut detail = vec![0u8; required as usize];
        // SP_DEVICE_INTERFACE_DETAIL_DATA_W.cbSize is 8 on 64-bit Windows and 6 on 32-bit Windows.
        let cb_size: u32 = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
        detail[0..4].copy_from_slice(&cb_size.to_ne_bytes());
        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set, &interface_data, detail.as_mut_ptr() as *mut _,
                required, std::ptr::null_mut(), std::ptr::null_mut()
            )
        };
        if ok == 0 {
            diagnostics.push(format!("Interface #{index}: SetupDiGetDeviceInterfaceDetailW failed (Win32 error {})", unsafe { GetLastError() }));
            continue;
        }
        interface_paths_found += 1;

        let path_words = unsafe {
            std::slice::from_raw_parts(detail.as_ptr().add(4) as *const u16, (required as usize - 4) / 2)
        };
        let path_len = path_words.iter().position(|word| *word == 0).unwrap_or(path_words.len());
        let device_path = String::from_utf16_lossy(&path_words[..path_len]);
        let path_wide = wide(&device_path);

        let device_handle = unsafe {
            CreateFileW(
                path_wide.as_ptr(), GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(),
                OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED, std::ptr::null_mut()
            )
        };
        if device_handle == INVALID_HANDLE_VALUE {
            diagnostics.push(format!("Interface #{index}: CreateFileW failed (Win32 error {}). DevicePath={}", unsafe { GetLastError() }, device_path));
            continue;
        }

        let mut winusb_handle: WinUsbHandle = std::ptr::null_mut();
        let initialized = unsafe { WinUsb_Initialize(device_handle, &mut winusb_handle) };
        if initialized == 0 {
            let error = unsafe { GetLastError() };
            diagnostics.push(format!("Interface #{index}: WinUsb_Initialize failed (Win32 error {error})"));
            unsafe { CloseHandle(device_handle); }
            continue;
        }

        let mut descriptor: UsbInterfaceDescriptor = unsafe { std::mem::zeroed() };
        let queried = unsafe { WinUsb_QueryInterfaceSettings(winusb_handle, 0, &mut descriptor) };
        if queried == 0 {
            diagnostics.push(format!("Interface #{index}: WinUsb_QueryInterfaceSettings(alt=0) failed (Win32 error {})", unsafe { GetLastError() }));
        } else {
            let mut endpoints = Vec::new();
            for pipe_index in 0..descriptor.endpoint_count {
                let mut pipe: WinUsbPipeInformation = unsafe { std::mem::zeroed() };
                if unsafe { WinUsb_QueryPipe(winusb_handle, 0, pipe_index, &mut pipe) } == 0 {
                    continue;
                }
                let transfer_type = match pipe.pipe_type {
                    0 => "Control",
                    1 => "Isochronous",
                    2 => "Bulk",
                    3 => "Interrupt",
                    _ => "Unknown",
                };
                endpoints.push(EndpointInfo {
                    index: pipe_index,
                    address: format!("0x{:02X}", pipe.pipe_id),
                    direction: if pipe.pipe_id & 0x80 != 0 { "IN" } else { "OUT" },
                    transfer_type,
                    max_packet_size: pipe.maximum_packet_size,
                    interval: pipe.interval,
                });
            }
            inspections.push(InterfaceInspection {
                interface_number: descriptor.interface_number,
                alternate_setting: descriptor.alternate_setting,
                endpoint_count: descriptor.endpoint_count,
                interface_class: format!("0x{:02X}", descriptor.interface_class),
                interface_subclass: format!("0x{:02X}", descriptor.interface_subclass),
                interface_protocol: format!("0x{:02X}", descriptor.interface_protocol),
                endpoints,
            });
        }

        unsafe {
            WinUsb_Free(winusb_handle);
            CloseHandle(device_handle);
        }
    }

    unsafe { SetupDiDestroyDeviceInfoList(set); }
    if inspections.is_empty() {
        if interface_paths_found == 0 {
            return Err(format!("Apple USBMUX GUID에 해당하는 장치 인터페이스 경로를 찾지 못했습니다. SetupDiEnumDeviceInterfaces 검색 결과가 0개입니다. GUID 등록 상태와 iPhone 연결 상태를 확인하세요. {}", diagnostics.join(" | ")));
        }
        return Err(format!("GUID 인터페이스 경로 {interface_paths_found}개를 찾았지만 WinUSB 조회에 성공하지 못했습니다. {}", diagnostics.join(" | ")));
    }
    Ok(InspectionResult { interface_path_found: interface_paths_found > 0, interfaces: inspections })
}

#[cfg(not(windows))]
fn inspect_winusb_interfaces() -> Result<InspectionResult, String> {
    Err("WinUSB inspection is currently supported on Windows only.".to_string())
}


#[derive(Serialize)]
struct UsbMuxProbeResult {
    connected: bool,
    response_message: Option<String>,
    result_number: Option<u64>,
    device_count: Option<usize>,
    response_preview: String,
}

fn probe_usbmux() -> Result<UsbMuxProbeResult, String> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX TCP connect failed: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("Set read timeout failed: {e}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("Set write timeout failed: {e}"))?;

    // libusbmuxd protocol v1: MESSAGE_PLIST (8), XML plist payload.
    let mut request = plist::Dictionary::new();
    request.insert("BundleID".into(), plist::Value::String("com.diplay.windows".into()));
    request.insert("ClientVersionString".into(), plist::Value::String("DiPlay Windows 0.1".into()));
    request.insert("MessageType".into(), plist::Value::String("ListDevices".into()));
    request.insert("ProgName".into(), plist::Value::String("DiPlay".into()));
    request.insert("kLibUSBMuxVersion".into(), plist::Value::Integer(3.into()));

    let mut payload = Vec::new();
    plist::to_writer_xml(&mut payload, &plist::Value::Dictionary(request))
        .map_err(|e| format!("USBMUX plist serialization failed: {e}"))?;

    let total_len = 16usize.checked_add(payload.len())
        .ok_or_else(|| "USBMUX request length overflow".to_string())?;
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&(total_len as u32).to_le_bytes());
    header.extend_from_slice(&1u32.to_le_bytes()); // plist protocol
    header.extend_from_slice(&8u32.to_le_bytes()); // MESSAGE_PLIST
    header.extend_from_slice(&1u32.to_le_bytes()); // tag
    stream.write_all(&header).and_then(|_| stream.write_all(&payload))
        .map_err(|e| format!("USBMUX ListDevices send failed: {e}"))?;

    let mut response_header = [0u8; 16];
    stream.read_exact(&mut response_header)
        .map_err(|e| format!("USBMUX response header read failed: {e}"))?;
    let response_len = u32::from_le_bytes(response_header[0..4].try_into().unwrap()) as usize;
    let response_version = u32::from_le_bytes(response_header[4..8].try_into().unwrap());
    let response_type = u32::from_le_bytes(response_header[8..12].try_into().unwrap());
    let response_tag = u32::from_le_bytes(response_header[12..16].try_into().unwrap());
    if response_len < 16 || response_len > 1024 * 1024 {
        return Err(format!("Invalid USBMUX response length: {response_len}"));
    }
    let mut response_payload = vec![0u8; response_len - 16];
    stream.read_exact(&mut response_payload)
        .map_err(|e| format!("USBMUX response payload read failed: {e}"))?;

    if response_type != 8 {
        return Err(format!(
            "USBMUX returned non-plist packet: type={response_type}, version={response_version}, tag={response_tag}, payload={} bytes",
            response_payload.len()
        ));
    }

    let value: plist::Value = plist::from_reader(std::io::Cursor::new(&response_payload))
        .map_err(|e| format!("USBMUX response plist parse failed: {e}; payload={}", String::from_utf8_lossy(&response_payload)))?;
    let dict = value.as_dictionary()
        .ok_or_else(|| format!("USBMUX response is not a plist dictionary: {value:?}"))?;
    let response_message = dict.get("MessageType").and_then(|v| v.as_string()).map(str::to_owned);
    let result_number = dict.get("Number").and_then(|v| v.as_unsigned_integer());
    let device_count = dict.get("DeviceList").and_then(|v| v.as_array()).map(|v| v.len());

    Ok(UsbMuxProbeResult {
        connected: true,
        response_message,
        result_number,
        device_count,
        response_preview: format!("{value:?}"),
    })
}


#[derive(Serialize)]
struct LockdownProbeResult {
    connected: bool,
    device_id: u64,
    port: u16,
    lockdown_type: Option<String>,
    response_preview: String,
}

fn read_usbmux_plist(stream: &mut std::net::TcpStream) -> Result<plist::Value, String> {
    use std::io::Read;
    let mut header = [0u8; 16];
    stream.read_exact(&mut header)
        .map_err(|e| format!("USBMUX response header read failed: {e}"))?;
    let length = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let message = u32::from_le_bytes(header[8..12].try_into().unwrap());
    if length < 16 || length > 1024 * 1024 {
        return Err(format!("Invalid USBMUX response length: {length}"));
    }
    if version != 1 || message != 8 {
        return Err(format!("Unexpected USBMUX response header: version={version}, message={message}"));
    }
    let mut payload = vec![0u8; length - 16];
    stream.read_exact(&mut payload)
        .map_err(|e| format!("USBMUX response payload read failed: {e}"))?;
    plist::from_reader(std::io::Cursor::new(payload))
        .map_err(|e| format!("USBMUX response plist parse failed: {e}"))
}

fn write_usbmux_plist(stream: &mut std::net::TcpStream, tag: u32, value: plist::Value) -> Result<(), String> {
    use std::io::Write;
    let mut payload = Vec::new();
    plist::to_writer_xml(&mut payload, &value)
        .map_err(|e| format!("USBMUX plist serialization failed: {e}"))?;
    let length = 16usize.checked_add(payload.len())
        .ok_or_else(|| "USBMUX request length overflow".to_string())?;
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&(length as u32).to_le_bytes());
    header.extend_from_slice(&1u32.to_le_bytes());
    header.extend_from_slice(&8u32.to_le_bytes());
    header.extend_from_slice(&tag.to_le_bytes());
    stream.write_all(&header).and_then(|_| stream.write_all(&payload))
        .map_err(|e| format!("USBMUX request write failed: {e}"))
}

fn make_usbmux_message(message_type: &str) -> plist::Value {
    let mut dict = plist::Dictionary::new();
    dict.insert("BundleID".into(), plist::Value::String("com.diplay.windows".into()));
    dict.insert("ClientVersionString".into(), plist::Value::String("DiPlay Windows 0.1".into()));
    dict.insert("MessageType".into(), plist::Value::String(message_type.into()));
    dict.insert("ProgName".into(), plist::Value::String("DiPlay".into()));
    dict.insert("kLibUSBMuxVersion".into(), plist::Value::Integer(3.into()));
    plist::Value::Dictionary(dict)
}

fn lockdown_query_type() -> Result<LockdownProbeResult, String> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    // First ask the Apple usbmux service for the currently attached USB device ID.
    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list_response = read_usbmux_plist(&mut mux)?;
    let list_dict = list_response.as_dictionary()
        .ok_or_else(|| "USBMUX ListDevices response is not a dictionary".to_string())?;
    let device_list = list_dict.get("DeviceList").and_then(|v| v.as_array())
        .ok_or_else(|| format!("USBMUX response has no DeviceList: {list_response:?}"))?;
    let selected = device_list.iter().find_map(|entry| {
        let entry_dict = entry.as_dictionary()?;
        let props = entry_dict.get("Properties")?.as_dictionary()?;
        let connection = props.get("ConnectionType")?.as_string()?;
        if connection != "USB" { return None; }
        entry_dict.get("DeviceID").and_then(|v| v.as_unsigned_integer())
    }).ok_or_else(|| "No USB-connected iPhone was found in DeviceList".to_string())?;
    drop(mux);

    // Connect to lockdownd on TCP port 62078 through usbmux.
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(7))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut connect_request = make_usbmux_message("Connect");
    let connect_dict = connect_request.as_dictionary_mut().unwrap();
    connect_dict.insert("DeviceID".into(), plist::Value::Integer(selected.into()));
    // libusbmuxd encodes PortNumber with htons() in the plist integer.
    connect_dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect_request)?;

    let connect_response = read_usbmux_plist(&mut mux)?;
    let response_dict = connect_response.as_dictionary()
        .ok_or_else(|| format!("USBMUX Connect response is not a dictionary: {connect_response:?}"))?;
    let result = response_dict.get("Number").and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response has no result number: {connect_response:?}"))?;
    if result != 0 {
        return Err(format!("USBMUX Connect to device {selected} port 62078 failed with result {result}: {connect_response:?}"));
    }

    // Lockdown uses a 4-byte big-endian length followed by an XML plist.
    let mut query = plist::Dictionary::new();
    query.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    query.insert("Request".into(), plist::Value::String("QueryType".into()));
    let mut query_payload = Vec::new();
    plist::to_writer_xml(&mut query_payload, &plist::Value::Dictionary(query))
        .map_err(|e| format!("Lockdown QueryType plist serialization failed: {e}"))?;
    let query_len = u32::try_from(query_payload.len())
        .map_err(|_| "Lockdown request is too large".to_string())?;
    mux.write_all(&query_len.to_be_bytes()).and_then(|_| mux.write_all(&query_payload))
        .map_err(|e| format!("Lockdown QueryType request write failed: {e}"))?;

    use std::io::Read;
    let mut length_bytes = [0u8; 4];
    mux.read_exact(&mut length_bytes)
        .map_err(|e| format!("Lockdown response length read failed: {e}"))?;
    let response_len = u32::from_be_bytes(length_bytes) as usize;
    if response_len == 0 || response_len > 1024 * 1024 {
        return Err(format!("Invalid Lockdown response length: {response_len}"));
    }
    let mut response_payload = vec![0u8; response_len];
    mux.read_exact(&mut response_payload)
        .map_err(|e| format!("Lockdown response body read failed: {e}"))?;
    let response: plist::Value = plist::from_reader(std::io::Cursor::new(&response_payload))
        .map_err(|e| format!("Lockdown response plist parse failed: {e}; payload={}", String::from_utf8_lossy(&response_payload)))?;
    let response_dict = response.as_dictionary()
        .ok_or_else(|| format!("Lockdown response is not a dictionary: {response:?}"))?;
    let lockdown_type = response_dict.get("Type").and_then(|v| v.as_string()).map(str::to_owned);

    Ok(LockdownProbeResult {
        connected: true,
        device_id: selected,
        port: 62078,
        lockdown_type,
        response_preview: format!("{response:?}"),
    })
}

fn print_response<T: Serialize>(event: &'static str, status: &'static str, message: Option<String>, devices: Option<T>) {
    let response = Response { event, status, message, devices };
    if let Ok(json) = serde_json::to_string(&response) { println!("{json}"); }
}

fn main() {
    print_response::<Vec<Device>>("backend", "ready", None, None);
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match line.trim() {
            "ping" => print_response::<Vec<Device>>("pong", "ready", None, None),
            "usb:list" => match enumerate_usb() {
                Ok(devices) => print_response("usb:list", "ok", None, Some(devices)),
                Err(error) => print_response::<Vec<Device>>("usb:list", "error", Some(error), None),
            },
            "usb:inspect" => match inspect_winusb_interfaces() {
                Ok(result) => print_response("usb:inspect", "ok", None, Some(result)),
                Err(error) => print_response::<InspectionResult>("usb:inspect", "error", Some(error), None),
            },
            "usbmux:probe" => match probe_usbmux() {
                Ok(result) => print_response("usbmux:probe", "ok", None, Some(result)),
                Err(error) => print_response::<UsbMuxProbeResult>("usbmux:probe", "error", Some(error), None),
            },
            "lockdown:query-type" => match lockdown_query_type() {
                Ok(result) => print_response("lockdown:query-type", "ok", None, Some(result)),
                Err(error) => print_response::<LockdownProbeResult>("lockdown:query-type", "error", Some(error), None),
            },
            _ => print_response::<Vec<Device>>("error", "unknown-command", Some("Unknown command".to_string()), None),
        }
    }
}
