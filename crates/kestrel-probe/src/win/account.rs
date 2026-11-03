//! Who owns the desktop and who runs Kestrel, from Windows' tokens and sessions. Read-only: the shell's process is
//! opened with the query-only right and its token with TOKEN_QUERY, and nothing is changed. The shell is not a game,
//! so this opens no handle to one.
use crate::parse;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, SidTypeUser, TokenUser, PSID, SID_NAME_USE,
    TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTSDomainName, WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
    WTS_CURRENT_SERVER_HANDLE, WTS_INFO_CLASS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

/// The account at the screen, by SID: the owner of the shell on Kestrel's own desktop, else the user signed in to
/// Kestrel's session. None when neither can be named.
pub(crate) fn signed_in() -> Option<String> {
    shell_owner().or_else(session_user)
}

/// The account Kestrel's own process runs as, by SID.
pub(crate) fn this_process() -> Option<String> {
    // SAFETY: returns the pseudo-handle for this process, which needs no closing.
    token_user(unsafe { GetCurrentProcess() })
}

/// The owner of the shell (Explorer) on this desktop, by SID. None when the desktop has no shell or Windows won't let
/// its process be opened for a query.
fn shell_owner() -> Option<String> {
    // SAFETY: takes no arguments; the answer is a window handle, or null when there is no shell.
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    // SAFETY: `shell` is a window handle and `process_id` is a valid place for the id of the process that owns it.
    let thread = unsafe { GetWindowThreadProcessId(shell, &mut process_id) };
    if thread == 0 || process_id == 0 {
        return None;
    }
    // SAFETY: opens the process by its id with the query-only right and no inheritance; the handle is closed when
    // `process` is dropped.
    let opened = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    let process = OpenHandle::new(opened)?;
    token_user(process.0)
}

/// The user signed in to Kestrel's own session, by SID: the way to ask when the shell can't be. Windows names the
/// session's user and domain, and the SID comes from that name.
fn session_user() -> Option<String> {
    // SAFETY: takes no arguments.
    let process_id = unsafe { GetCurrentProcessId() };
    let mut session = 0u32;
    // SAFETY: `session` is a valid place for the id of the session this process is in.
    if unsafe { ProcessIdToSessionId(process_id, &mut session) } == 0 {
        return None;
    }
    let user = session_text(session, WTSUserName)?;
    // A session that names no domain is looked up by its user name alone.
    let domain = session_text(session, WTSDomainName).unwrap_or_default();
    sid_of(&parse::account_name(&domain, &user)?)
}

/// One piece of text about a session (its user name, its domain) from this PC's session host. None if Windows has no
/// such session or won't say.
fn session_text(session: u32, info: WTS_INFO_CLASS) -> Option<String> {
    let mut buffer: PWSTR = std::ptr::null_mut();
    let mut bytes = 0u32;
    // SAFETY: the session host of this PC and one of its sessions; `buffer` and `bytes` are valid places for the
    // answers.
    let asked = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session,
            info,
            &mut buffer,
            &mut bytes,
        )
    };
    if asked == 0 || buffer.is_null() {
        return None;
    }
    // SAFETY: Windows wrote `bytes` bytes at `buffer`, a NUL-terminated string for the classes this is asked for.
    let units = unsafe { std::slice::from_raw_parts(buffer, bytes as usize / 2) };
    let text = parse::exe_name(units);
    // SAFETY: the buffer came from WTSQuerySessionInformationW, which says to free it with WTSFreeMemory, and it is
    // not used again.
    unsafe { WTSFreeMemory(buffer.cast()) };
    Some(text)
}

/// The SID of the user account called `name` (`DOMAIN\user`, or just the user). None if Windows doesn't know the name
/// or it isn't a user's.
fn sid_of(name: &str) -> Option<String> {
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let (mut sid_size, mut domain_size) = (0u32, 0u32);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: `name` is NUL-terminated. With null buffers of size 0 the call only reports how much room the answers
    // need; it fails with "buffer too small", which is the answer wanted.
    unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut sid_size,
            std::ptr::null_mut(),
            &mut domain_size,
            &mut kind,
        )
    };
    if sid_size == 0 {
        return None;
    }
    // A SID needs 4-byte alignment, which a buffer of u64s has. The domain's name isn't wanted but must be given room.
    let mut sid = vec![0u64; (sid_size as usize).div_ceil(8)];
    let mut domain_size = domain_size.max(1);
    let mut domain = vec![0u16; domain_size as usize];
    // SAFETY: `sid` is writable for the `sid_size` bytes and `domain` for the `domain_size` units passed with them;
    // `name` is NUL-terminated.
    let found = unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut kind,
        )
    };
    if found == 0 || kind != SidTypeUser {
        return None;
    }
    sid_string(sid.as_mut_ptr().cast())
}

/// The SID of the account whose token the process `process` runs with. `process` must be open for a query, or be the
/// pseudo-handle for this process.
fn token_user(process: HANDLE) -> Option<String> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is open for a query and `token` is a valid place for the new handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    // From here on, dropping `token` closes it, however this function ends.
    let token = OpenHandle::new(token)?;
    let mut size = 0u32;
    // SAFETY: with a null buffer of size 0 the call only reports how much room the answer needs.
    unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut size) };
    if (size as usize) < size_of::<TOKEN_USER>() {
        return None;
    }
    // The answer starts with a TOKEN_USER, which holds a pointer, so the buffer is a Vec<u64>: 8-byte aligned.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let mut written = 0u32;
    // SAFETY: `token` is open for TOKEN_QUERY and `buffer` is writable for the `size` bytes passed with it.
    let read = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut written,
        )
    };
    if read == 0 {
        return None;
    }
    // SAFETY: Windows wrote a TOKEN_USER at the start of the buffer, which is long enough for one (checked above) and
    // aligned for it. The SID it points to is stored after it, in this same buffer.
    let sid = unsafe { buffer.as_ptr().cast::<TOKEN_USER>().read() }
        .User
        .Sid;
    sid_string(sid)
}

/// A SID as text, `S-1-5-21-…`.
fn sid_string(sid: PSID) -> Option<String> {
    let mut text: PWSTR = std::ptr::null_mut();
    // SAFETY: `sid` points to a valid SID and `text` is a valid place for the string Windows allocates.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 || text.is_null() {
        return None;
    }
    let mut length = 0;
    // SAFETY: Windows ends the string with a NUL, so every unit up to it can be read.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the `length` units before that NUL are readable and initialised.
    let units = unsafe { std::slice::from_raw_parts(text, length) };
    let spelled = String::from_utf16_lossy(units);
    // SAFETY: the string was allocated by ConvertSidToStringSidW, which says to free it with LocalFree, and it is not
    // used again.
    unsafe { LocalFree(text.cast()) };
    Some(spelled)
}

/// A handle from OpenProcess or OpenProcessToken, closed when dropped.
struct OpenHandle(HANDLE);

impl OpenHandle {
    /// None for the null handle that these calls give when they fail.
    fn new(handle: HANDLE) -> Option<Self> {
        (!handle.is_null()).then_some(Self(handle))
    }
}

impl Drop for OpenHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by the call that gave it to `new` and is closed only here.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A SID's text: `S-1-` and then numbers between dashes (`S-1-5-21-1-2-3-1001`).
    fn is_sid_text(text: &str) -> bool {
        text.strip_prefix("S-1-").is_some_and(|rest| {
            rest.split('-')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        })
    }

    /// This test process's SID as `whoami` reports it.
    fn whoami_sid() -> String {
        let output = Command::new("whoami")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .expect("whoami should run");
        // One line, like: "desktop-4f7k2\gabs","S-1-5-21-1-2-3-1001". The name is in the console's code page, but
        // the SID, which is all that is read, is plain ASCII.
        let text = String::from_utf8_lossy(&output.stdout);
        let (_, sid) = text
            .trim()
            .rsplit_once(',')
            .expect("whoami should print a name and a SID");
        sid.trim_matches('"').to_string()
    }

    #[test]
    fn this_process_is_named_by_the_sid_whoami_reports() {
        let sid = whoami_sid();
        assert!(is_sid_text(&sid), "whoami said {sid}");
        assert_eq!(this_process().as_deref(), Some(sid.as_str()));
    }

    #[test]
    fn a_user_name_resolves_to_its_sid() {
        // The name the user signed in with, as Windows put it in this process's environment.
        let var = |name| std::env::var(name).unwrap_or_else(|_| panic!("{name} should be set"));
        let name = format!("{}\\{}", var("USERDOMAIN"), var("USERNAME"));
        assert_eq!(sid_of(&name), this_process(), "looking up {name}");
        // A name on this PC that no account has.
        let nobody = format!("{}\\kestrel-no-such-user", var("COMPUTERNAME"));
        assert_eq!(sid_of(&nobody), None);
    }

    #[test]
    fn a_sid_is_spelled_the_way_windows_writes_it() {
        // A SID is a revision, the number of sub-authorities, a 6-byte big-endian authority and the sub-authorities
        // as little-endian u32s. Built as u32 words, which gives the 4-byte alignment a SID needs.
        let spell = |bytes: &[u8]| {
            let mut words: Vec<u32> = bytes
                .chunks(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            sid_string(words.as_mut_ptr().cast())
        };
        assert_eq!(
            spell(&[1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0]).as_deref(),
            Some("S-1-5-18")
        );
        let account = [
            1, 5, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0xE9, 0x03, 0,
            0,
        ];
        assert_eq!(spell(&account).as_deref(), Some("S-1-5-21-1-2-3-1001"));
    }

    #[test]
    fn both_ways_of_finding_the_signed_in_account_agree() {
        // Either way can come back empty: a PC with no desktop session, like a build runner, may have no shell. When
        // both answer they name the same account, and every answer is a SID.
        let (shell, session) = (shell_owner(), session_user());
        for sid in shell.iter().chain(&session) {
            assert!(is_sid_text(sid), "not a SID: {sid}");
        }
        if let (Some(shell), Some(session)) = (&shell, &session) {
            assert_eq!(shell, session);
        }
    }

    #[test]
    fn a_session_that_does_not_exist_has_no_user() {
        assert_eq!(session_text(0x00FF_FFFF, WTSUserName), None);
    }
}
