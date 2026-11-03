//! The mouse setting in force, through `SystemParametersInfoW`: three integers, [threshold 1, threshold 2,
//! acceleration]. `SPI_GETMOUSE` needs read access to the window station and `SPI_SETMOUSE` write access, and a
//! non-interactive window station can refuse either, so both can fail for reasons that have nothing to do with the
//! arguments.
use std::io;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendNotifyMessageW, SystemParametersInfoW, HWND_BROADCAST, SPIF_UPDATEINIFILE, SPI_GETMOUSE,
    SPI_SETMOUSE, WM_SETTINGCHANGE,
};

use crate::parse;
use crate::system::Read;

/// The mouse in force, with Windows' own error if it won't say.
pub(super) fn read() -> io::Result<[i32; 3]> {
    let mut values = [0i32; 3];
    // SAFETY: for SPI_GETMOUSE the third argument points to an array of three integers, which `values` is, and the
    // others must be 0.
    if unsafe { SystemParametersInfoW(SPI_GETMOUSE, 0, values.as_mut_ptr().cast(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(values)
}

pub(super) fn live() -> Read<[i32; 3]> {
    match read() {
        Ok(values) => Read::Value(values),
        Err(why) => Read::Failed(parse::failed_read("the mouse setting", &why)),
    }
}

/// Sets the mouse in force. `persist` also has Windows save it in the caller's profile. Open windows are then told.
///
/// `SPIF_SENDCHANGE` isn't used, though it's the usual way to tell them: its broadcast can block on one unresponsive
/// top-level window, and Apply, Undo and Revert everything make this call under the journal's lock, so a hung window
/// would stall them with no timeout, and the uninstaller's undo with them.
pub(super) fn set_live(mut values: [i32; 3], persist: bool) -> io::Result<()> {
    let flags = if persist { SPIF_UPDATEINIFILE } else { 0 };
    // SAFETY: for SPI_SETMOUSE the third argument points to an array of three integers, which `values` is, and Windows
    // only reads it. The second must be 0.
    let set = unsafe { SystemParametersInfoW(SPI_SETMOUSE, 0, values.as_mut_ptr().cast(), flags) };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    tell_open_windows();
    Ok(())
}

/// Hands every top-level window the news. It doesn't wait for another thread's windows, and the threads that call this
/// have none of their own, so it returns at once. Its result is ignored because a failed notification isn't a failed
/// change: the mouse is set by now, and the most that's lost is an open Mouse Properties showing the old state until it
/// is opened again.
fn tell_open_windows() {
    // SAFETY: HWND_BROADCAST is a valid target, and both message arguments are plain numbers: a message sent this way
    // can't carry a pointer.
    unsafe { SendNotifyMessageW(HWND_BROADCAST, WM_SETTINGCHANGE, SPI_SETMOUSE as usize, 0) };
}
