//! The mouse setting in force, through `SystemParametersInfoW`: three integers, [threshold 1, threshold 2,
//! acceleration]. `SPI_GETMOUSE` needs read access to the window station and `SPI_SETMOUSE` write access, and a
//! non-interactive window station can refuse either, so both can fail for reasons that have nothing to do with the
//! arguments.
use std::io;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETMOUSE, SPI_SETMOUSE,
};

use crate::parse;
use crate::system::Read;

pub(super) fn live() -> Read<[i32; 3]> {
    let mut values = [0i32; 3];
    // SAFETY: for SPI_GETMOUSE the third argument points to an array of three integers, which `values` is, and the
    // others must be 0.
    let read = unsafe { SystemParametersInfoW(SPI_GETMOUSE, 0, values.as_mut_ptr().cast(), 0) };
    if read == 0 {
        return Read::Failed(parse::failed_read(
            "the mouse setting",
            &io::Error::last_os_error(),
        ));
    }
    Read::Value(values)
}

/// Sets the mouse in force and tells every window. `persist` also has Windows save it in the caller's profile.
pub(super) fn set_live(mut values: [i32; 3], persist: bool) -> io::Result<()> {
    let flags = if persist {
        SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
    } else {
        SPIF_SENDCHANGE
    };
    // SAFETY: for SPI_SETMOUSE the third argument points to an array of three integers, which `values` is, and Windows
    // only reads it. The second must be 0.
    let set = unsafe { SystemParametersInfoW(SPI_SETMOUSE, 0, values.as_mut_ptr().cast(), flags) };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
