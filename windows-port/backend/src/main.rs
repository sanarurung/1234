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

    // HDEVINFO is an isize in windows-sys 0.59; INVALID_HANDLE_VALUE is -1.
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
            _ => print_response::<Vec<Device>>("error", "unknown-command", Some("Unknown command".to_string()), None),
        }
    }
}
