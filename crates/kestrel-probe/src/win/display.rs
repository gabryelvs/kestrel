//! Monitors: current and highest refresh rate at the current resolution, via EnumDisplaySettings.
use crate::parse;
use kestrel_core::{gpu, DisplayInfo, GpuVendor, SystemSnapshot};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, ENUM_CURRENT_SETTINGS,
};

/// One display mode of `device` (a device name from EnumDisplayDevices): `index` counts modes, or is ENUM_CURRENT_SETTINGS.
fn mode(device: &[u16], index: u32) -> Option<DEVMODEW> {
    if !device.contains(&0) {
        return None; // not NUL-terminated, so Windows could read past the end
    }
    let mut m = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `device` holds a NUL, so Windows stops reading inside it; `m` is a valid DEVMODEW whose dmSize says
    // how much Windows may write.
    (unsafe { EnumDisplaySettingsW(device.as_ptr(), index, &mut m) } != 0).then_some(m)
}

pub fn fill(s: &mut SystemSnapshot) {
    let mut displays = Vec::new();
    for index in 0u32.. {
        let mut dev = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: a null device name asks for display adapters; `dev` is a valid DISPLAY_DEVICEW whose cb says
        // how much Windows may write.
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut dev, 0) } == 0 {
            break;
        }
        if dev.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let Some(current) = mode(&dev.DeviceName, ENUM_CURRENT_SETTINGS) else {
            continue;
        };
        let max_hz = (0u32..)
            .map_while(|i| mode(&dev.DeviceName, i))
            .filter(|m| {
                m.dmPelsWidth == current.dmPelsWidth && m.dmPelsHeight == current.dmPelsHeight
            })
            .map(|m| m.dmDisplayFrequency)
            .max()
            .unwrap_or(current.dmDisplayFrequency);
        let adapter = parse::exe_name(&dev.DeviceString);
        displays.push(DisplayInfo {
            name: parse::exe_name(&dev.DeviceName),
            is_virtual: gpu::vendor_from("", &adapter) == GpuVendor::Virtual,
            adapter,
            current_hz: current.dmDisplayFrequency,
            max_hz: max_hz.max(current.dmDisplayFrequency),
            width: current.dmPelsWidth,
            height: current.dmPelsHeight,
            is_primary: dev.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
        });
    }
    s.displays = displays;
}
