use serde::Serialize;
use std::io::{self, BufRead, Write};
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


#[derive(Serialize)]
struct PairingPreflightResult {
    connected: bool,
    device_id: u64,
    unique_device_id: Option<String>,
    device_name: Option<String>,
    product_version: Option<String>,
    device_public_key_bytes: Option<usize>,
    has_device_public_key: bool,
}

fn lockdown_send_receive<S: std::io::Read + std::io::Write>(stream: &mut S, request: plist::Value) -> Result<plist::Value, String> {
    use std::io::{Read, Write};
    let mut payload = Vec::new();
    plist::to_writer_xml(&mut payload, &request).map_err(|e| format!("Lockdown plist serialization failed: {e}"))?;
    let length = u32::try_from(payload.len()).map_err(|_| "Lockdown request is too large".to_string())?;
    stream.write_all(&length.to_be_bytes()).and_then(|_| stream.write_all(&payload))
        .map_err(|e| format!("Lockdown request write failed: {e}"))?;
    let mut length_bytes = [0u8; 4];
    stream.read_exact(&mut length_bytes).map_err(|e| format!("Lockdown response length read failed: {e}"))?;
    let response_len = u32::from_be_bytes(length_bytes) as usize;
    if response_len == 0 || response_len > 1024 * 1024 { return Err(format!("Invalid Lockdown response length: {response_len}")); }
    let mut response_payload = vec![0u8; response_len];
    stream.read_exact(&mut response_payload).map_err(|e| format!("Lockdown response body read failed: {e}"))?;
    plist::from_reader(std::io::Cursor::new(response_payload)).map_err(|e| format!("Lockdown response plist parse failed: {e}"))
}

fn lockdown_get_value(stream: &mut std::net::TcpStream, key: &str) -> Result<plist::Value, String> {
    let mut request = plist::Dictionary::new();
    request.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    request.insert("Request".into(), plist::Value::String("GetValue".into()));
    request.insert("Key".into(), plist::Value::String(key.into()));
    let response = lockdown_send_receive(stream, plist::Value::Dictionary(request))?;
    let dict = response.as_dictionary().ok_or_else(|| format!("GetValue({key}) response is not a dictionary: {response:?}"))?;
    if let Some(error) = dict.get("Error").and_then(|v| v.as_string()) { return Err(format!("Lockdown GetValue({key}) rejected: {error}")); }
    dict.get("Value").cloned().ok_or_else(|| format!("Lockdown GetValue({key}) response has no Value: {response:?}"))
}

fn lockdown_pairing_preflight() -> Result<PairingPreflightResult, String> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list = read_usbmux_plist(&mut mux)?;
    let list_dict = list.as_dictionary().ok_or_else(|| "USBMUX ListDevices response is not a dictionary".to_string())?;
    let devices = list_dict.get("DeviceList").and_then(|v| v.as_array()).ok_or_else(|| format!("USBMUX response has no DeviceList: {list:?}"))?;
    let selected = devices.iter().find_map(|entry| {
        let dict = entry.as_dictionary()?;
        let props = dict.get("Properties")?.as_dictionary()?;
        if props.get("ConnectionType")?.as_string()? != "USB" { return None; }
        dict.get("DeviceID")?.as_unsigned_integer()
    }).ok_or_else(|| "No USB-connected iPhone was found".to_string())?;
    drop(mux);

    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(7))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut connect = make_usbmux_message("Connect");
    let connect_dict = connect.as_dictionary_mut().unwrap();
    connect_dict.insert("DeviceID".into(), plist::Value::Integer(selected.into()));
    connect_dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect)?;
    let connected = read_usbmux_plist(&mut mux)?;
    let result = connected.as_dictionary().and_then(|d| d.get("Number")).and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response is invalid: {connected:?}"))?;
    if result != 0 { return Err(format!("USBMUX Connect to lockdownd failed with result {result}")); }

    let mut query = plist::Dictionary::new();
    query.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    query.insert("Request".into(), plist::Value::String("QueryType".into()));
    let query_response = lockdown_send_receive(&mut mux, plist::Value::Dictionary(query))?;
    let lockdown_type = query_response.as_dictionary().and_then(|d| d.get("Type")).and_then(|v| v.as_string());
    if lockdown_type != Some("com.apple.mobile.lockdown") { return Err(format!("Unexpected Lockdown QueryType response: {query_response:?}")); }

    let udid = lockdown_get_value(&mut mux, "UniqueDeviceID")?.as_string().map(str::to_owned);
    let name = lockdown_get_value(&mut mux, "DeviceName")?.as_string().map(str::to_owned);
    let version = lockdown_get_value(&mut mux, "ProductVersion")?.as_string().map(str::to_owned);
    let public_key = lockdown_get_value(&mut mux, "DevicePublicKey")?;
    let public_key_bytes = public_key.as_data().map(|data| data.len());
    if public_key_bytes.unwrap_or(0) == 0 { return Err(format!("DevicePublicKey was not returned as non-empty plist data: {public_key:?}")); }

    Ok(PairingPreflightResult { connected: true, device_id: selected, unique_device_id: udid, device_name: name, product_version: version, device_public_key_bytes: public_key_bytes, has_device_public_key: true })
}


#[derive(Serialize)]
struct PairingMaterialResult {
    prepared: bool,
    device_id: u64,
    unique_device_id: String,
    host_id: String,
    system_buid: String,
    root_certificate_bytes: usize,
    host_certificate_bytes: usize,
    device_certificate_bytes: usize,
    protected_record_path: String,
}

#[cfg(windows)]
fn dpapi_protect(input: &[u8]) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct DataBlob { cb_data: u32, pb_data: *mut u8 }
    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(data_in: *const DataBlob, description: *const u16, optional_entropy: *const DataBlob,
            reserved: *mut c_void, prompt: *mut c_void, flags: u32, data_out: *mut DataBlob) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" { fn LocalFree(memory: *mut c_void) -> *mut c_void; }

    let mut source = input.to_vec();
    let input_blob = DataBlob { cb_data: source.len() as u32, pb_data: source.as_mut_ptr() };
    let mut output_blob = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
    let ok = unsafe { CryptProtectData(&input_blob, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 1, &mut output_blob) };
    if ok == 0 { return Err(format!("Windows DPAPI CryptProtectData failed: {}", std::io::Error::last_os_error())); }
    let protected = unsafe { std::slice::from_raw_parts(output_blob.pb_data, output_blob.cb_data as usize).to_vec() };
    unsafe { LocalFree(output_blob.pb_data as *mut c_void); }
    Ok(protected)
}

#[cfg(not(windows))]
fn dpapi_protect(_input: &[u8]) -> Result<Vec<u8>, String> {
    Err("Pairing records can only be protected with Windows DPAPI on Windows.".into())
}


#[cfg(windows)]
fn dpapi_unprotect(input: &[u8]) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct DataBlob { cb_data: u32, pb_data: *mut u8 }
    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptUnprotectData(data_in: *const DataBlob, description: *mut *mut u16, optional_entropy: *const DataBlob,
            reserved: *mut c_void, prompt: *mut c_void, flags: u32, data_out: *mut DataBlob) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" { fn LocalFree(memory: *mut c_void) -> *mut c_void; }

    if input.len() > u32::MAX as usize {
        return Err("DPAPI input exceeds the Windows DATA_BLOB size limit".into());
    }
    let mut source = input.to_vec();
    let input_blob = DataBlob { cb_data: source.len() as u32, pb_data: source.as_mut_ptr() };
    let mut output_blob = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
    let ok = unsafe { CryptUnprotectData(&input_blob, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), 1, &mut output_blob) };
    if ok == 0 {
        return Err(format!("Windows DPAPI CryptUnprotectData failed: {}", std::io::Error::last_os_error()));
    }
    let clear = unsafe { std::slice::from_raw_parts(output_blob.pb_data, output_blob.cb_data as usize).to_vec() };
    unsafe { LocalFree(output_blob.pb_data as *mut c_void); }
    Ok(clear)
}

#[cfg(not(windows))]
fn dpapi_unprotect(_input: &[u8]) -> Result<Vec<u8>, String> {
    Err("Pairing records can only be unprotected with Windows DPAPI on Windows.".into())
}

fn prepare_pairing_materials() -> Result<PairingMaterialResult, String> {
    use openssl::asn1::{Asn1Integer, Asn1Time};
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::pkey::{PKey, Private};
    use openssl::rsa::Rsa;
    use openssl::x509::extension::{BasicConstraints, KeyUsage, SubjectKeyIdentifier};
    use openssl::x509::{X509NameBuilder, X509};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    use uuid::Uuid;

    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list = read_usbmux_plist(&mut mux)?;
    let list_dict = list.as_dictionary().ok_or_else(|| "USBMUX ListDevices response is not a dictionary".to_string())?;
    let devices = list_dict.get("DeviceList").and_then(|v| v.as_array()).ok_or_else(|| "USBMUX response has no DeviceList".to_string())?;
    let selected_entry = devices.iter().find(|entry| {
        entry.as_dictionary().and_then(|d| d.get("Properties")).and_then(|v| v.as_dictionary())
            .and_then(|p| p.get("ConnectionType")).and_then(|v| v.as_string()) == Some("USB")
    }).ok_or_else(|| "No USB-connected iPhone was found".to_string())?;
    let selected_dict = selected_entry.as_dictionary().unwrap();
    let device_id = selected_dict.get("DeviceID").and_then(|v| v.as_unsigned_integer()).ok_or("USB device has no DeviceID")?;
    let props = selected_dict.get("Properties").and_then(|v| v.as_dictionary()).ok_or("USB device has no Properties")?;
    let udid = props.get("SerialNumber").and_then(|v| v.as_string()).ok_or("USB device has no SerialNumber/UDID")?.to_string();
    drop(mux);

    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut connect = make_usbmux_message("Connect");
    let connect_dict = connect.as_dictionary_mut().unwrap();
    connect_dict.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
    connect_dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect)?;
    let connected = read_usbmux_plist(&mut mux)?;
    let connect_result = connected.as_dictionary().and_then(|d| d.get("Number")).and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response invalid: {connected:?}"))?;
    if connect_result != 0 { return Err(format!("USBMUX Connect failed: {connected:?}")); }

    let mut query = plist::Dictionary::new();
    query.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    query.insert("Request".into(), plist::Value::String("QueryType".into()));
    let query_result = lockdown_send_receive(&mut mux, plist::Value::Dictionary(query))?;
    if query_result.as_dictionary().and_then(|d| d.get("Type")).and_then(|v| v.as_string()) != Some("com.apple.mobile.lockdown") {
        return Err(format!("Unexpected Lockdown service response: {query_result:?}"));
    }
    let device_public_key_value = lockdown_get_value(&mut mux, "DevicePublicKey")?;
    let device_public_key_pem = device_public_key_value.as_data().ok_or("DevicePublicKey is not plist data")?;
    // lockdownd exposes DevicePublicKey as PEM (libimobiledevice parses it with PEM_read_bio_PUBKEY).
    let device_public_key = match PKey::public_key_from_pem(device_public_key_pem) {
        Ok(key) => key,
        Err(spki_error) => {
            let rsa = Rsa::public_key_from_pem(device_public_key_pem)
                .map_err(|rsa_error| format!("Cannot parse iPhone DevicePublicKey as PUBLIC KEY ({spki_error}) or RSA PUBLIC KEY ({rsa_error})"))?;
            PKey::from_rsa(rsa).map_err(|e| format!("Cannot wrap iPhone RSA public key: {e}"))?
        }
    };

    // Use a versioned path so the first corrected record never overwrites the earlier
    // preflight artifact. Existing v2 records are decrypted and fully checked before reuse.
    let base = std::env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA is not available".to_string())?;
    let dir = std::path::PathBuf::from(base).join("DiPlay").join("Pairing");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create protected pairing directory: {e}"))?;
    let path = dir.join(format!("{udid}.pairrecord.v2.dpapi"));

    if path.exists() {
        use openssl::pkey::PKey;
        use openssl::x509::X509;
        let protected = std::fs::read(&path).map_err(|e| format!("Cannot read existing protected PairRecord: {e}"))?;
        let clear = dpapi_unprotect(&protected)?;
        let value: plist::Value = plist::from_reader(std::io::Cursor::new(clear))
            .map_err(|e| format!("Existing PairRecord plist is invalid: {e}"))?;
        let dict = value.as_dictionary().ok_or("Existing PairRecord is not a plist dictionary")?;
        let get_data = |key: &str| -> Result<Vec<u8>, String> {
            dict.get(key).and_then(|v| v.as_data()).map(|v| v.to_vec())
                .ok_or_else(|| format!("Existing PairRecord is missing binary field {key}"))
        };
        let get_string = |key: &str| -> Result<String, String> {
            dict.get(key).and_then(|v| v.as_string()).map(str::to_owned)
                .ok_or_else(|| format!("Existing PairRecord is missing string field {key}"))
        };

        let saved_udid = get_string("UniqueDeviceID")?;
        if saved_udid != udid {
            return Err("Existing PairRecord UDID does not match the connected iPhone; refusing to reuse it".into());
        }
        let saved_device_key = get_data("DevicePublicKey")?;
        if saved_device_key != device_public_key_pem {
            return Err("The connected iPhone DevicePublicKey differs from the saved PairRecord; refusing unsafe reuse".into());
        }

        let root_pem = get_data("RootCertificate")?;
        let host_pem = get_data("HostCertificate")?;
        let device_pem = get_data("DeviceCertificate")?;
        let root_key_pem = get_data("RootPrivateKey")?;
        let host_key_pem = get_data("HostPrivateKey")?;
        let root_cert = X509::from_pem(&root_pem).map_err(|e| format!("Saved RootCertificate is invalid: {e}"))?;
        let host_cert = X509::from_pem(&host_pem).map_err(|e| format!("Saved HostCertificate is invalid: {e}"))?;
        let device_cert = X509::from_pem(&device_pem).map_err(|e| format!("Saved DeviceCertificate is invalid: {e}"))?;
        let root_key = PKey::private_key_from_pem(&root_key_pem).map_err(|e| format!("Saved RootPrivateKey is invalid: {e}"))?;
        let host_key = PKey::private_key_from_pem(&host_key_pem).map_err(|e| format!("Saved HostPrivateKey is invalid: {e}"))?;
        let root_public_key = root_cert.public_key().map_err(|e| e.to_string())?;
        let host_public_key = host_cert.public_key().map_err(|e| e.to_string())?;
        let device_cert_public_key = device_cert.public_key().map_err(|e| e.to_string())?;

        if !root_cert.verify(&root_public_key).map_err(|e| format!("Root certificate signature check failed: {e}"))? {
            return Err("Saved RootCertificate is not self-signed by its own public key".into());
        }
        if !host_cert.verify(&root_public_key).map_err(|e| format!("Host certificate signature check failed: {e}"))? {
            return Err("Saved HostCertificate is not signed by the saved root".into());
        }
        if !device_cert.verify(&root_public_key).map_err(|e| format!("Device certificate signature check failed: {e}"))? {
            return Err("Saved DeviceCertificate is not signed by the saved root".into());
        }
        if !root_public_key.public_eq(&root_key) || !host_public_key.public_eq(&host_key) {
            return Err("Saved certificate public keys do not match their private keys".into());
        }
        if !device_cert_public_key.public_eq(&device_public_key) {
            return Err("Saved DeviceCertificate does not contain the connected iPhone public key".into());
        }
        for (label, cert) in [("Root", &root_cert), ("Host", &host_cert), ("Device", &device_cert)] {
            let serial = cert.serial_number().to_bn().map_err(|e| format!("{label} certificate serial is invalid: {e}"))?;
            if serial.num_bits() == 0 {
                return Err(format!("Saved {label} certificate has a non-positive serial number; refusing reuse"));
            }
        }

        let host_id = get_string("HostID")?;
        let system_buid = get_string("SystemBUID")?;
        return Ok(PairingMaterialResult {
            prepared: true,
            device_id,
            unique_device_id: udid,
            host_id,
            system_buid,
            root_certificate_bytes: root_pem.len(),
            host_certificate_bytes: host_pem.len(),
            device_certificate_bytes: device_pem.len(),
            protected_record_path: path.display().to_string(),
        });
    }

    let rsa_root = Rsa::generate(2048).map_err(|e| format!("RSA root key generation failed: {e}"))?;
    let root_key: PKey<Private> = PKey::from_rsa(rsa_root).map_err(|e| format!("Root key conversion failed: {e}"))?;
    let rsa_host = Rsa::generate(2048).map_err(|e| format!("RSA host key generation failed: {e}"))?;
    let host_key: PKey<Private> = PKey::from_rsa(rsa_host).map_err(|e| format!("Host key conversion failed: {e}"))?;

    let now = Asn1Time::days_from_now(0).map_err(|e| e.to_string())?;
    let expiry = Asn1Time::days_from_now(3650).map_err(|e| e.to_string())?;

    let mut root_name_builder = X509NameBuilder::new().map_err(|e| e.to_string())?;
    root_name_builder.append_entry_by_text("CN", "DiPlay Windows Pairing Root").map_err(|e| e.to_string())?;
    let root_name = root_name_builder.build();
    let mut root_builder = X509::builder().map_err(|e| e.to_string())?;
    root_builder.set_version(2).map_err(|e| e.to_string())?;
    let root_serial_bn = BigNum::from_u32(1).map_err(|e| e.to_string())?;
    let root_serial = Asn1Integer::from_bn(&root_serial_bn).map_err(|e| e.to_string())?;
    root_builder.set_serial_number(&root_serial).map_err(|e| e.to_string())?;
    root_builder.set_subject_name(&root_name).map_err(|e| e.to_string())?;
    root_builder.set_issuer_name(&root_name).map_err(|e| e.to_string())?;
    root_builder.set_pubkey(&root_key).map_err(|e| e.to_string())?;
    root_builder.set_not_before(&now).map_err(|e| e.to_string())?;
    root_builder.set_not_after(&expiry).map_err(|e| e.to_string())?;
    root_builder.append_extension(BasicConstraints::new().critical().ca().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    root_builder.append_extension(KeyUsage::new().critical().key_cert_sign().crl_sign().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    root_builder.sign(&root_key, MessageDigest::sha256()).map_err(|e| e.to_string())?;
    let root_cert = root_builder.build();

    let mut host_name_builder = X509NameBuilder::new().map_err(|e| e.to_string())?;
    host_name_builder.append_entry_by_text("CN", "DiPlay Windows Host").map_err(|e| e.to_string())?;
    let host_name = host_name_builder.build();
    let mut host_builder = X509::builder().map_err(|e| e.to_string())?;
    host_builder.set_version(2).map_err(|e| e.to_string())?;
    let host_serial_bn = BigNum::from_u32(1).map_err(|e| e.to_string())?;
    let host_serial = Asn1Integer::from_bn(&host_serial_bn).map_err(|e| e.to_string())?;
    host_builder.set_serial_number(&host_serial).map_err(|e| e.to_string())?;
    host_builder.set_subject_name(&host_name).map_err(|e| e.to_string())?;
    host_builder.set_issuer_name(root_cert.subject_name()).map_err(|e| e.to_string())?;
    host_builder.set_pubkey(&host_key).map_err(|e| e.to_string())?;
    host_builder.set_not_before(&now).map_err(|e| e.to_string())?;
    host_builder.set_not_after(&expiry).map_err(|e| e.to_string())?;
    host_builder.append_extension(BasicConstraints::new().critical().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    host_builder.append_extension(KeyUsage::new().critical().digital_signature().key_encipherment().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    host_builder.sign(&root_key, MessageDigest::sha256()).map_err(|e| e.to_string())?;
    let host_cert = host_builder.build();

    let mut device_name_builder = X509NameBuilder::new().map_err(|e| e.to_string())?;
    device_name_builder.append_entry_by_text("CN", "iPhone Device").map_err(|e| e.to_string())?;
    let device_name = device_name_builder.build();
    let mut device_builder = X509::builder().map_err(|e| e.to_string())?;
    device_builder.set_version(2).map_err(|e| e.to_string())?;
    let device_serial_bn = BigNum::from_u32(3).map_err(|e| e.to_string())?;
    let device_serial = Asn1Integer::from_bn(&device_serial_bn).map_err(|e| e.to_string())?;
    device_builder.set_serial_number(&device_serial).map_err(|e| e.to_string())?;
    device_builder.set_subject_name(&device_name).map_err(|e| e.to_string())?;
    device_builder.set_issuer_name(root_cert.subject_name()).map_err(|e| e.to_string())?;
    device_builder.set_pubkey(&device_public_key).map_err(|e| e.to_string())?;
    device_builder.set_not_before(&now).map_err(|e| e.to_string())?;
    device_builder.set_not_after(&expiry).map_err(|e| e.to_string())?;
    device_builder.append_extension(BasicConstraints::new().critical().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    device_builder.append_extension(KeyUsage::new().critical().digital_signature().key_encipherment().build().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let subject_key_id = {
        let context = device_builder.x509v3_context(Some(&root_cert), None);
        SubjectKeyIdentifier::new().build(&context).map_err(|e| e.to_string())?
    };
    device_builder.append_extension(subject_key_id).map_err(|e| e.to_string())?;
    device_builder.sign(&root_key, MessageDigest::sha256()).map_err(|e| e.to_string())?;
    let device_cert = device_builder.build();

    let host_id = Uuid::new_v4().to_string().to_uppercase();
    let system_buid = Uuid::new_v4().to_string().to_uppercase();
    let mut record = plist::Dictionary::new();
    record.insert("DeviceCertificate".into(), plist::Value::Data(device_cert.to_pem().map_err(|e| e.to_string())?));
    record.insert("HostCertificate".into(), plist::Value::Data(host_cert.to_pem().map_err(|e| e.to_string())?));
    record.insert("RootCertificate".into(), plist::Value::Data(root_cert.to_pem().map_err(|e| e.to_string())?));
    record.insert("HostPrivateKey".into(), plist::Value::Data(host_key.private_key_to_pem_pkcs8().map_err(|e| e.to_string())?));
    record.insert("RootPrivateKey".into(), plist::Value::Data(root_key.private_key_to_pem_pkcs8().map_err(|e| e.to_string())?));
    record.insert("DevicePublicKey".into(), plist::Value::Data(device_public_key_pem.to_vec()));
    record.insert("HostID".into(), plist::Value::String(host_id.clone()));
    record.insert("SystemBUID".into(), plist::Value::String(system_buid.clone()));
    record.insert("UniqueDeviceID".into(), plist::Value::String(udid.clone()));

    let mut serialized = Vec::new();
    plist::to_writer_binary(&mut serialized, &plist::Value::Dictionary(record))
        .map_err(|e| format!("PairRecord serialization failed: {e}"))?;
    let protected = dpapi_protect(&serialized)?;
    std::fs::write(&path, protected).map_err(|e| format!("Cannot save DPAPI-protected PairRecord: {e}"))?;

    Ok(PairingMaterialResult {
        prepared: true, device_id, unique_device_id: udid, host_id, system_buid,
        root_certificate_bytes: root_cert.to_pem().map_err(|e| e.to_string())?.len(),
        host_certificate_bytes: host_cert.to_pem().map_err(|e| e.to_string())?.len(),
        device_certificate_bytes: device_cert.to_pem().map_err(|e| e.to_string())?.len(),
        protected_record_path: path.display().to_string()
    })
}


#[derive(Serialize)]
struct PairingRequestResult {
    device_id: u64,
    unique_device_id: String,
    pairing_state: String,
    response_preview: String,
    message: String,
}

fn send_lockdown_pair_request() -> Result<PairingRequestResult, String> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    // Ensure a valid, device-matched, DPAPI-protected PairRecord exists before
    // making the trust-changing request.
    let prepared = prepare_pairing_materials()?;
    let protected = std::fs::read(&prepared.protected_record_path)
        .map_err(|e| format!("Cannot read protected PairRecord: {e}"))?;
    let clear = dpapi_unprotect(&protected)?;
    let mut stored: plist::Value = plist::from_reader(std::io::Cursor::new(clear))
        .map_err(|e| format!("Cannot parse protected PairRecord: {e}"))?;
    let stored_dict = stored.as_dictionary()
        .ok_or("Protected PairRecord is not a dictionary")?;

    // Only public certificates and identifiers are sent to the iPhone.
    // HostPrivateKey and RootPrivateKey remain local and DPAPI-protected.
    let mut pair_record = plist::Dictionary::new();
    for key in ["DeviceCertificate", "HostCertificate", "RootCertificate"] {
        let value = stored_dict.get(key)
            .and_then(|v| v.as_data())
            .ok_or_else(|| format!("PairRecord is missing binary field {key}"))?;
        pair_record.insert(key.into(), plist::Value::Data(value.to_vec()));
    }
    for key in ["HostID", "SystemBUID"] {
        let value = stored_dict.get(key)
            .and_then(|v| v.as_string())
            .ok_or_else(|| format!("PairRecord is missing string field {key}"))?;
        pair_record.insert(key.into(), plist::Value::String(value.to_string()));
    }

    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(12))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list = read_usbmux_plist(&mut mux)?;
    let devices = list.as_dictionary()
        .and_then(|d| d.get("DeviceList"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("USBMUX ListDevices response has no DeviceList: {list:?}"))?;
    let still_attached = devices.iter().any(|entry| {
        let Some(dict) = entry.as_dictionary() else { return false; };
        let Some(props) = dict.get("Properties").and_then(|v| v.as_dictionary()) else { return false; };
        props.get("ConnectionType").and_then(|v| v.as_string()) == Some("USB")
            && props.get("SerialNumber").and_then(|v| v.as_string()) == Some(prepared.unique_device_id.as_str())
    });
    if !still_attached {
        return Err("The iPhone used to create this PairRecord is no longer attached over USB".into());
    }
    drop(mux);

    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(20))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut connect = make_usbmux_message("Connect");
    let connect_dict = connect.as_dictionary_mut().unwrap();
    connect_dict.insert("DeviceID".into(), plist::Value::Integer(prepared.device_id.into()));
    connect_dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect)?;
    let connected = read_usbmux_plist(&mut mux)?;
    let connect_result = connected.as_dictionary()
        .and_then(|d| d.get("Number"))
        .and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response is invalid: {connected:?}"))?;
    if connect_result != 0 {
        return Err(format!("USBMUX Connect to lockdownd failed: {connected:?}"));
    }

    let mut request = plist::Dictionary::new();
    request.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    request.insert("PairRecord".into(), plist::Value::Dictionary(pair_record));
    request.insert("Request".into(), plist::Value::String("Pair".into()));
    request.insert("ProtocolVersion".into(), plist::Value::String("2".into()));

    let response = lockdown_send_receive(&mut mux, plist::Value::Dictionary(request))?;
    let response_dict = response.as_dictionary()
        .ok_or_else(|| format!("Lockdown Pair response is not a dictionary: {response:?}"))?;
    let error = response_dict.get("Error").and_then(|v| v.as_string());
    let response_request = response_dict.get("Request").and_then(|v| v.as_string());
    let result = response_dict.get("Result").and_then(|v| v.as_string());

    if let Some(error) = error {
        let (state, message) = match error {
            "PairingDialogResponsePending" => (
                "awaiting_user_approval",
                "iPhone에 신뢰 확인 창이 표시됐을 수 있습니다. iPhone 잠금을 해제하고 '이 컴퓨터를 신뢰'를 선택한 뒤 Pair 요청을 다시 실행하세요."
            ),
            "UserDeniedPairing" => (
                "denied",
                "iPhone에서 이 컴퓨터에 대한 신뢰를 거부했습니다. 원할 경우 기기에서 다시 연결해 신뢰 절차를 시작하세요."
            ),
            "PasswordProtected" => (
                "device_locked",
                "iPhone 잠금을 해제한 뒤 Pair 요청을 다시 실행하세요."
            ),
            _ => return Err(format!("Lockdown Pair rejected the request: {error}; response={response:?}")),
        };
        return Ok(PairingRequestResult {
            device_id: prepared.device_id,
            unique_device_id: prepared.unique_device_id,
            pairing_state: state.into(),
            response_preview: format!("{response:?}"),
            message: message.into(),
        });
    }

    if response_request != Some("Pair") {
        return Err(format!("Unexpected Lockdown Pair response (Request={response_request:?}): {response:?}"));
    }
    if result.is_some_and(|value| value != "Success") {
        return Err(format!("Lockdown Pair did not succeed: {response:?}"));
    }

    // Persist the EscrowBag returned by lockdownd. Apple clients retain it in
    // the local pair record for later escrow-protected service requests.
    if let Some(escrow) = response_dict.get("EscrowBag").and_then(|v| v.as_data()) {
        let stored_dict = stored.as_dictionary_mut()
            .ok_or("Protected PairRecord is not a mutable dictionary")?;
        stored_dict.insert("EscrowBag".into(), plist::Value::Data(escrow.to_vec()));
        let mut serialized = Vec::new();
        plist::to_writer_xml(&mut serialized, &stored)
            .map_err(|e| format!("Updated PairRecord serialization failed: {e}"))?;
        let updated_protected = dpapi_protect(&serialized)?;
        let temp_path = format!("{}.tmp", prepared.protected_record_path);
        std::fs::write(&temp_path, updated_protected)
            .map_err(|e| format!("Cannot write updated protected PairRecord: {e}"))?;
        std::fs::rename(&temp_path, &prepared.protected_record_path)
            .or_else(|_| {
                std::fs::remove_file(&prepared.protected_record_path)?;
                std::fs::rename(&temp_path, &prepared.protected_record_path)
            })
            .map_err(|e| format!("Cannot replace protected PairRecord: {e}"))?;
    } else {
        return Err("Pair succeeded but lockdownd did not return a binary EscrowBag; PairRecord was not finalized".into());
    }

    Ok(PairingRequestResult {
        device_id: prepared.device_id,
        unique_device_id: prepared.unique_device_id,
        pairing_state: "paired".into(),
        response_preview: format!("{response:?}"),
        message: "iPhone이 Pair 요청을 성공으로 응답했습니다. 다음 단계에서 ValidatePair로 신뢰 상태를 확인할 수 있습니다.".into(),
    })
}


#[derive(Serialize)]
struct ValidatePairResult {
    device_id: u64,
    unique_device_id: String,
    validated: bool,
    escrow_bag_bytes: usize,
    response_preview: String,
    message: String,
}

fn validate_lockdown_pairing() -> Result<ValidatePairResult, String> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let prepared = prepare_pairing_materials()?;
    let protected = std::fs::read(&prepared.protected_record_path)
        .map_err(|e| format!("Cannot read protected PairRecord: {e}"))?;
    let clear = dpapi_unprotect(&protected)?;
    let stored: plist::Value = plist::from_reader(std::io::Cursor::new(clear))
        .map_err(|e| format!("Cannot parse protected PairRecord: {e}"))?;
    let stored_dict = stored.as_dictionary()
        .ok_or("Protected PairRecord is not a dictionary")?;
    let escrow_bytes = stored_dict.get("EscrowBag").and_then(|v| v.as_data())
        .map(|v| v.len())
        .ok_or("PairRecord has no EscrowBag. Run the Pair request and approve it on the iPhone first.")?;

    // Match libimobiledevice: send the complete stored public PairRecord,
    // retaining EscrowBag and any device-specific public fields. Only local
    // private keys must be excluded from the request.
    let mut pair_record = stored_dict.clone();
    pair_record.remove("HostPrivateKey");
    pair_record.remove("RootPrivateKey");

    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list = read_usbmux_plist(&mut mux)?;
    let devices = list.as_dictionary().and_then(|d| d.get("DeviceList")).and_then(|v| v.as_array())
        .ok_or_else(|| format!("USBMUX ListDevices response has no DeviceList: {list:?}"))?;
    let attached = devices.iter().any(|entry| {
        let Some(dict) = entry.as_dictionary() else { return false; };
        let Some(props) = dict.get("Properties").and_then(|v| v.as_dictionary()) else { return false; };
        props.get("ConnectionType").and_then(|v| v.as_string()) == Some("USB")
            && props.get("SerialNumber").and_then(|v| v.as_string()) == Some(prepared.unique_device_id.as_str())
    });
    if !attached { return Err("The paired iPhone is not attached over USB".into()); }
    drop(mux);

    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(15))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut connect = make_usbmux_message("Connect");
    let dict = connect.as_dictionary_mut().unwrap();
    dict.insert("DeviceID".into(), plist::Value::Integer(prepared.device_id.into()));
    dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect)?;
    let connected = read_usbmux_plist(&mut mux)?;
    let code = connected.as_dictionary().and_then(|d| d.get("Number")).and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response invalid: {connected:?}"))?;
    if code != 0 { return Err(format!("USBMUX Connect to lockdownd failed: {connected:?}")); }

    let mut request = plist::Dictionary::new();
    request.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    request.insert("PairRecord".into(), plist::Value::Dictionary(pair_record));
    request.insert("Request".into(), plist::Value::String("ValidatePair".into()));
    request.insert("ProtocolVersion".into(), plist::Value::String("2".into()));
    let response = lockdown_send_receive(&mut mux, plist::Value::Dictionary(request))?;
    let response_dict = response.as_dictionary()
        .ok_or_else(|| format!("Lockdown ValidatePair response is not a dictionary: {response:?}"))?;
    if let Some(error) = response_dict.get("Error").and_then(|v| v.as_string()) {
        return Err(format!("Lockdown ValidatePair rejected the PairRecord: {error}; response={response:?}"));
    }
    if response_dict.get("Request").and_then(|v| v.as_string()) != Some("ValidatePair")
        || response_dict.get("Result").and_then(|v| v.as_string()) != Some("Success") {
        return Err(format!("Unexpected ValidatePair response: {response:?}"));
    }

    Ok(ValidatePairResult {
        device_id: prepared.device_id,
        unique_device_id: prepared.unique_device_id,
        validated: true,
        escrow_bag_bytes: escrow_bytes,
        response_preview: format!("{response:?}"),
        message: "ValidatePair 성공: iPhone이 이 HostID를 신뢰하는 것으로 확인됐습니다.".into(),
    })
}


#[derive(Serialize)]
struct StartSessionResult {
    device_id: u64,
    unique_device_id: String,
    session_id: String,
    ssl_enabled: bool,
    tls_established: bool,
    device_name: String,
    message: String,
}

fn start_lockdown_session() -> Result<StartSessionResult, String> {
    use openssl::pkey::PKey;
    use openssl::ssl::{Ssl, SslContextBuilder, SslMethod, SslStream, SslVerifyMode};
    use openssl::x509::X509;
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let prepared = prepare_pairing_materials()?;
    let protected = std::fs::read(&prepared.protected_record_path)
        .map_err(|e| format!("Cannot read protected PairRecord: {e}"))?;
    let clear = dpapi_unprotect(&protected)?;
    let stored: plist::Value = plist::from_reader(std::io::Cursor::new(clear))
        .map_err(|e| format!("Cannot parse protected PairRecord: {e}"))?;
    let record = stored.as_dictionary().ok_or("Protected PairRecord is not a dictionary")?;

    let get_data = |key: &str| -> Result<Vec<u8>, String> {
        record.get(key).and_then(|v| v.as_data()).map(|v| v.to_vec())
            .ok_or_else(|| format!("PairRecord is missing binary field {key}"))
    };
    let get_string = |key: &str| -> Result<String, String> {
        record.get(key).and_then(|v| v.as_string()).map(str::to_owned)
            .ok_or_else(|| format!("PairRecord is missing string field {key}"))
    };

    let host_id = get_string("HostID")?;
    let system_buid = get_string("SystemBUID")?;
    let host_cert = X509::from_pem(&get_data("HostCertificate")?)
        .map_err(|e| format!("Saved HostCertificate is invalid: {e}"))?;
    let host_key = PKey::private_key_from_pem(&get_data("HostPrivateKey")?)
        .map_err(|e| format!("Saved HostPrivateKey is invalid: {e}"))?;
    let root_cert = X509::from_pem(&get_data("RootCertificate")?)
        .map_err(|e| format!("Saved RootCertificate is invalid: {e}"))?;

    let address: SocketAddr = "127.0.0.1:27015".parse().unwrap();
    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX connect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(15))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;

    write_usbmux_plist(&mut mux, 1, make_usbmux_message("ListDevices"))?;
    let list = read_usbmux_plist(&mut mux)?;
    let devices = list.as_dictionary().and_then(|d| d.get("DeviceList"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("USBMUX ListDevices response has no DeviceList: {list:?}"))?;
    let selected = devices.iter().find(|entry| {
        let Some(d) = entry.as_dictionary() else { return false; };
        let Some(props) = d.get("Properties").and_then(|v| v.as_dictionary()) else { return false; };
        props.get("ConnectionType").and_then(|v| v.as_string()) == Some("USB")
            && props.get("SerialNumber").and_then(|v| v.as_string()) == Some(prepared.unique_device_id.as_str())
    }).ok_or("The paired iPhone is not attached over USB")?;
    let device_id = selected.as_dictionary().and_then(|d| d.get("DeviceID"))
        .and_then(|v| v.as_unsigned_integer()).ok_or("USB device has no DeviceID")?;
    drop(mux);

    let mut mux = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| format!("Apple USBMUX reconnect failed: {e}"))?;
    mux.set_read_timeout(Some(Duration::from_secs(20))).map_err(|e| e.to_string())?;
    mux.set_write_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;

    let mut connect = make_usbmux_message("Connect");
    let connect_dict = connect.as_dictionary_mut().unwrap();
    connect_dict.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
    connect_dict.insert("PortNumber".into(), plist::Value::Integer(u16::to_be(62078).into()));
    write_usbmux_plist(&mut mux, 2, connect)?;
    let connected = read_usbmux_plist(&mut mux)?;
    let connect_code = connected.as_dictionary().and_then(|d| d.get("Number"))
        .and_then(|v| v.as_unsigned_integer())
        .ok_or_else(|| format!("USBMUX Connect response is invalid: {connected:?}"))?;
    if connect_code != 0 {
        return Err(format!("USBMUX Connect to lockdownd failed: {connected:?}"));
    }

    let mut request = plist::Dictionary::new();
    request.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
    request.insert("Request".into(), plist::Value::String("StartSession".into()));
    request.insert("HostID".into(), plist::Value::String(host_id));
    request.insert("SystemBUID".into(), plist::Value::String(system_buid));

    let response = lockdown_send_receive(&mut mux, plist::Value::Dictionary(request))?;
    let response_dict = response.as_dictionary()
        .ok_or_else(|| format!("Lockdown StartSession response is not a dictionary: {response:?}"))?;
    if let Some(error) = response_dict.get("Error").and_then(|v| v.as_string()) {
        return Err(format!("Lockdown StartSession rejected the host: {error}; response={response:?}"));
    }
    if response_dict.get("Request").and_then(|v| v.as_string()) != Some("StartSession") {
        return Err(format!("Unexpected Lockdown StartSession response: {response:?}"));
    }
    let session_id = response_dict.get("SessionID").and_then(|v| v.as_string())
        .ok_or_else(|| format!("StartSession response has no SessionID: {response:?}"))?.to_string();
    let ssl_enabled = matches!(response_dict.get("EnableSessionSSL"), Some(plist::Value::Boolean(true)));

    let device_name;
    let tls_established;
    if ssl_enabled {
        let mut context = SslContextBuilder::new(SslMethod::tls_client())
            .map_err(|e| format!("TLS context creation failed: {e}"))?;
        context.set_certificate(&host_cert)
            .map_err(|e| format!("Cannot load HostCertificate for TLS: {e}"))?;
        context.set_private_key(&host_key)
            .map_err(|e| format!("Cannot load HostPrivateKey for TLS: {e}"))?;
        context.check_private_key()
            .map_err(|e| format!("HostCertificate and HostPrivateKey do not match: {e}"))?;
        context.cert_store_mut().add_cert(root_cert)
            .map_err(|e| format!("Cannot add RootCertificate to TLS trust store: {e}"))?;
        context.set_verify(SslVerifyMode::PEER);
        let context = context.build();
        let ssl = Ssl::new(&context).map_err(|e| format!("TLS session creation failed: {e}"))?;
        let mut tls = SslStream::new(ssl, mux)
            .map_err(|e| format!("TLS stream creation failed: {e}"))?;
        tls.connect().map_err(|e| format!("Lockdown TLS handshake failed: {e}"))?;
        tls_established = true;
        device_name = lockdown_get_value(&mut tls, "DeviceName")?
            .as_string().unwrap_or("iPhone".into()).to_string();

        let mut stop = plist::Dictionary::new();
        stop.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
        stop.insert("Request".into(), plist::Value::String("StopSession".into()));
        stop.insert("SessionID".into(), plist::Value::String(session_id.clone()));
        let _ = lockdown_send_receive(&mut tls, plist::Value::Dictionary(stop));
    } else {
        device_name = lockdown_get_value(&mut mux, "DeviceName")?
            .as_string().unwrap_or("iPhone".into()).to_string();
        tls_established = false;

        let mut stop = plist::Dictionary::new();
        stop.insert("Label".into(), plist::Value::String("DiPlay Windows".into()));
        stop.insert("Request".into(), plist::Value::String("StopSession".into()));
        stop.insert("SessionID".into(), plist::Value::String(session_id.clone()));
        let _ = lockdown_send_receive(&mut mux, plist::Value::Dictionary(stop));
    }

    Ok(StartSessionResult {
        device_id,
        unique_device_id: prepared.unique_device_id,
        session_id,
        ssl_enabled,
        tls_established,
        device_name: device_name.clone(),
        message: if ssl_enabled {
            format!("StartSession 및 TLS 핸드셰이크 성공. {device_name}의 DeviceName을 암호화된 Lockdown 세션에서 읽었습니다.")
        } else {
            format!("StartSession 성공. 기기가 TLS를 요구하지 않아 평문 Lockdown 세션으로 DeviceName을 읽었습니다.")
        },
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
            "lockdown:pairing-preflight" => match lockdown_pairing_preflight() {
                Ok(result) => print_response("lockdown:pairing-preflight", "ok", None, Some(result)),
                Err(error) => print_response::<PairingPreflightResult>("lockdown:pairing-preflight", "error", Some(error), None),
            },
            "pairing:prepare" => match prepare_pairing_materials() {
                Ok(result) => print_response("pairing:prepare", "ok", None, Some(result)),
                Err(error) => print_response::<PairingMaterialResult>("pairing:prepare", "error", Some(error), None),
            },
            "pairing:pair" => match send_lockdown_pair_request() {
                Ok(result) => print_response("pairing:pair", "ok", None, Some(result)),
                Err(error) => print_response::<PairingRequestResult>("pairing:pair", "error", Some(error), None),
            },
            "pairing:validate" => match validate_lockdown_pairing() {
                Ok(result) => print_response("pairing:validate", "ok", None, Some(result)),
                Err(error) => print_response::<ValidatePairResult>("pairing:validate", "error", Some(error), None),
            }
            "lockdown:start-session" => match start_lockdown_session() {
                Ok(result) => print_response("lockdown:start-session", "ok", None, Some(result)),
                Err(error) => print_response::<StartSessionResult>("lockdown:start-session", "error", Some(error), None),
            },,
            _ => print_response::<Vec<Device>>("error", "unknown-command", Some("Unknown command".to_string()), None),
        }
    }
}
