//! Monitor names and connections from Windows' display configuration (the names Settings → Display shows), joined to
//! the displays `display` listed by their GDI name. Read-only: QueryDisplayConfig and DisplayConfigGetDeviceInfo only
//! describe the current setup.
use super::problem;
use crate::parse::{self, MonitorPath};
use kestrel_core::{ProbeError, SystemSnapshot};
use windows_sys::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    match active_paths() {
        Some(paths) => {
            let monitors: Vec<MonitorPath> = paths.iter().filter_map(monitor_path).collect();
            parse::name_monitors(&mut s.displays, &monitors);
        }
        None => errors.push(problem(
            "displays",
            "couldn't read the display configuration (QueryDisplayConfig)",
        )),
    }
}

/// The paths from each desktop to the monitor that shows it, for the displays in use now.
fn active_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // A monitor plugged in between the two calls makes the answer outgrow the arrays, so then ask again.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: both counts are valid places for the answers.
        let status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        if path_count == 0 {
            return Some(Vec::new()); // no display in use (a PC without a monitor)
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: each array holds as many elements as its count says, and the topology id may be null because
        // QDC_DATABASE_CURRENT isn't asked for.
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        match status {
            ERROR_SUCCESS => {
                paths.truncate(path_count as usize);
                return Some(paths);
            }
            ERROR_INSUFFICIENT_BUFFER => continue,
            _ => return None,
        }
    }
    None
}

fn header(
    request: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    size: usize,
    adapter: LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: request,
        size: size as u32,
        adapterId: adapter,
        id,
    }
}

/// The path's desktop (GDI name) and its monitor's name and connection. None when Windows can't name the desktop; a
/// monitor it can't name keeps the connection the path itself gives.
fn monitor_path(path: &DISPLAYCONFIG_PATH_INFO) -> Option<MonitorPath> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
            path.sourceInfo.adapterId,
            path.sourceInfo.id,
        ),
        ..Default::default()
    };
    // SAFETY: the header names the request and the size of the whole packet, and the pointer covers the whole packet.
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut source).cast()) } != 0 {
        return None;
    }
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(),
            path.targetInfo.adapterId,
            path.targetInfo.id,
        ),
        ..Default::default()
    };
    // SAFETY: as above, for the target's packet.
    let named = unsafe { DisplayConfigGetDeviceInfo((&raw mut target).cast()) } == 0;
    let (monitor_name, technology) = if named {
        (
            parse::exe_name(&target.monitorFriendlyDeviceName),
            target.outputTechnology,
        )
    } else {
        (String::new(), path.targetInfo.outputTechnology)
    };
    Some(MonitorPath {
        gdi_name: parse::exe_name(&source.viewGdiDeviceName),
        monitor_name,
        output_technology: technology.cast_unsigned(),
    })
}
