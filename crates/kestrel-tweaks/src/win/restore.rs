//! System Protection and restore points on real Windows. GitHub's Windows machine is a Server image with no System
//! Restore, so only the reads here run before the owner's PC does. `create`, `turn_on` and `turn_off` change the
//! machine, and no test calls them.
//!
//! COM security can only be set once in a process and applies to all of it, so who sets it matters:
//!
//! - The helper's process sets Microsoft's, in `start_com`, as the first thing it does. COM is then running on its one
//!   thread, so the `wmi` crate uses it as it finds it and sets nothing of its own.
//! - The app's process never calls `start_com`. `protection` needs no COM at all. `turn_on` and `turn_off` use WMI as
//!   the probe does, on a thread of their own, because WebView2 owns the UI thread's COM and a connection can't leave
//!   its thread. On a thread with no COM state the `wmi` crate starts the multithreaded apartment and calls
//!   `CoInitializeSecurity` itself, with default values, and it goes on if that was already done. So the app's process
//!   has that call whether or not these run: its first scan makes it.
use std::collections::HashMap;
use std::fmt::Display;
use std::io;
use std::mem::{offset_of, transmute};
use std::ptr::{null, null_mut};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{FreeLibrary, LocalFree, ERROR_SUCCESS, HMODULE, MAX_PATH};
use windows_sys::Win32::Security::Authorization::{
    SetEntriesInAclW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SET_ACCESS, TRUSTEE_IS_GROUP,
    TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    CreateWellKnownSid, InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
    SetSecurityDescriptorGroup, SetSecurityDescriptorOwner, WinBuiltinAdministratorsSid,
    WinLocalServiceSid, WinLocalSystemSid, WinNetworkServiceSid, WinSelfSid, ACL, NO_INHERITANCE,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE, WELL_KNOWN_SID_TYPE,
};
use windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoInitializeSecurity, COINIT_MULTITHREADED, COM_RIGHTS_EXECUTE,
    COM_RIGHTS_EXECUTE_LOCAL, EOAC_DISABLE_AAA, EOAC_NO_CUSTOM_MARSHAL,
    RPC_C_AUTHN_LEVEL_PKT_PRIVACY, RPC_C_IMP_LEVEL_IDENTIFY,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Restore::{
    BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW, STATEMGRSTATUS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_sys::Win32::System::SystemServices::SECURITY_DESCRIPTOR_REVISION;
use wmi::{Variant, WMIConnection, WMIError};

use super::registry;
use crate::journal::RestoreOutcome;
use crate::parse;
use crate::restore::{
    create_error, make_point, newest_of, opened_a_new_point, protection_settles, CreateError,
    Point, RestorePoints, DESCRIPTION, POLL_WAIT,
};
use crate::system::Readiness;

/// Where WMI keeps System Restore.
const NAMESPACE: &str = r"root\default";
const LIST_POINTS: &str = "SELECT SequenceNumber, CreationTime FROM SystemRestore";

const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore";
const DISABLE_SR: &str = "DisableSR";
const DISABLE_CONFIG: &str = "DisableConfig";
/// The value that lists the volumes System Protection is on for. It's undocumented, and Sophia Script goes by it too.
const CLIENTS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\SPP\Clients";
const CLIENTS_VALUE: &str = "{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}";

/// What WMI answers about a class that isn't there, when asked for its objects and when asked for it.
const WBEM_E_INVALID_CLASS: i32 = 0x8004_1010_u32 as i32;
const WBEM_E_NOT_FOUND: i32 = 0x8004_1002_u32 as i32;

/// How long `Enable` or `Disable` may take to answer. Microsoft says `Enable` returns at once, and `Disable` has a
/// drive's restore points to delete.
const CALL_TIMEOUT: Duration = Duration::from_secs(180);

const NO_SYSTEM_RESTORE: &str = "This version of Windows has no System Restore.";
const NO_ANSWER: &str = "Windows didn't answer about System Protection.";

/// As `windows-sys` declares them, with the header's packing of 1. If they were otherwise, Windows would read and write
/// the wrong bytes.
const _: () = assert!(size_of::<RESTOREPOINTINFOW>() == 528);
const _: () = assert!(offset_of!(RESTOREPOINTINFOW, llSequenceNumber) == 8);
const _: () = assert!(offset_of!(RESTOREPOINTINFOW, szDescription) == 16);
const _: () = assert!(size_of::<STATEMGRSTATUS>() == 12);
const _: () = assert!(offset_of!(STATEMGRSTATUS, llSequenceNumber) == 4);

// System Protection.

/// Whether System Protection is on for the Windows drive. It only reads the registry and the volume's name, so it takes
/// no COM and is safe at any moment. See `parse::readiness` for what it comes to.
pub(super) fn protection() -> Readiness {
    parse::readiness(
        &registry::read_policy_dword(POLICY_KEY, DISABLE_SR),
        &registry::read_policy_dword(POLICY_KEY, DISABLE_CONFIG),
        &registry::machine_value_in_key(CLIENTS_KEY, CLIENTS_VALUE),
        system_volume().ok().as_deref(),
    )
}

/// Turns System Protection on for the Windows drive, and comes back once it reads on: Microsoft says `Enable` doesn't
/// wait, and the restore point right after this needs it.
pub(super) fn turn_on() -> io::Result<()> {
    switch("Enable", Readiness::Ready)
}

/// Turns System Protection off for the Windows drive, and comes back once it reads off, because the engine reads it
/// back straight away.
pub(super) fn turn_off() -> io::Result<()> {
    switch("Disable", Readiness::ProtectionOff)
}

fn switch(method: &'static str, wanted: Readiness) -> io::Result<()> {
    let drive = system_drive()?;
    let answer = on_a_thread_of_its_own(move || call(method, &drive))
        .ok_or_else(|| io::Error::other(NO_ANSWER))?
        .map_err(io::Error::other)?;
    parse::method_answer(method, answer).map_err(io::Error::other)?;
    protection_settles(wanted, protection, || thread::sleep(POLL_WAIT))
}

/// Runs WMI work on a fresh thread, which has no COM state, and waits for it. This runs under the journal's lock, so a
/// WMI that hangs mustn't stall everything else: past `CALL_TIMEOUT` the thread is left to end when it can. None means
/// that, or a panic there.
fn on_a_thread_of_its_own<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (answer, answered) = mpsc::channel();
    thread::spawn(move || {
        let _ = answer.send(work());
    });
    answered.recv_timeout(CALL_TIMEOUT).ok()
}

/// The class, by this type's name, which is how `wmi` finds a class for a method.
#[derive(Deserialize)]
struct SystemRestore;

/// The input of `Enable` and `Disable`. Microsoft's pages give them as `uint32 Enable([in] String Drive)`, with this
/// one parameter, and PowerShell's own `Enable-ComputerRestore` passes only this. `wmi` puts each field here into the
/// method's input, and that fails for a parameter the method doesn't have, so there is no asking it to wait.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct OnDrive<'a> {
    drive: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MethodOutput {
    return_value: u32,
}

/// Calls `SystemRestore.Enable` or `Disable` on a drive, and returns what it answers.
fn call(method: &str, drive: &str) -> Result<u32, String> {
    let output = WMIConnection::with_namespace_path(NAMESPACE)
        .and_then(|wmi| {
            wmi.exec_class_method::<SystemRestore, MethodOutput>(method, OnDrive { drive })
        })
        .map_err(|why| wmi_failure(&format!("call SystemRestore.{method}"), &why))?;
    Ok(output.return_value)
}

/// Why WMI wouldn't do something about System Restore, in a sentence. A class that isn't there is a Windows without it,
/// as on a Server, and PowerShell's own cmdlets read those two errors the same way.
fn wmi_failure(doing: &str, why: &WMIError) -> String {
    match why {
        WMIError::HResultError {
            hres: WBEM_E_INVALID_CLASS | WBEM_E_NOT_FOUND,
        } => NO_SYSTEM_RESTORE.to_string(),
        other => format!("Windows wouldn't {doing}: {other}."),
    }
}

/// The drive Windows is on, as `C:\`, from Windows and not from the environment, which is the caller's.
fn system_drive() -> io::Result<String> {
    let mut buffer = [0u16; MAX_PATH as usize];
    // SAFETY: Windows writes at most the given number of units to `buffer`, which has that many.
    let length = unsafe { GetSystemWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    let length = length as usize;
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    // Anything else is the size it would need, which is more than a Windows folder can be, and nothing was written.
    let Some(folder) = buffer.get(..length).filter(|_| length < buffer.len()) else {
        return Err(io::Error::other("The Windows folder's path is too long."));
    };
    let folder = String::from_utf16_lossy(folder);
    parse::system_drive(&folder)
        .ok_or_else(|| io::Error::other(format!("{folder} isn't on a lettered drive.")))
}

/// The volume the Windows drive is on, as `\\?\Volume{GUID}\`.
fn system_volume() -> io::Result<String> {
    let drive = parse::wide(&system_drive()?);
    // Microsoft says 50 is a reasonable size for the largest one there can be.
    let mut name = [0u16; 50];
    // SAFETY: `drive` is a terminated wide string ending in a backslash, as asked for, and Windows writes at most the
    // given number of units to `name`, which has that many.
    let found = unsafe {
        GetVolumeNameForVolumeMountPointW(drive.as_ptr(), name.as_mut_ptr(), name.len() as u32)
    };
    if found == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    Ok(String::from_utf16_lossy(&name[..end]))
}

// The helper.

/// The helper's whole job, in a process of its own, on that process's first thread.
pub(crate) fn make_point_here() -> RestoreOutcome {
    if let Err(reason) = start_com() {
        return RestoreOutcome::Failed { reason };
    }
    make_point(&WinPoints, DESCRIPTION, &mut || thread::sleep(POLL_WAIT))
}

/// What the helper would find on this PC, in one line, without asking for a restore point. It sets the whole process's
/// COM security, as the helper does, so it needs a process of its own too: only the round trip's own copy of itself
/// runs this, to get a real Windows to read these before the owner's PC. WMI is asked for something every Windows has
/// as well, because a Windows with no System Restore says so before it has sent back anything else.
pub fn helper_reads() -> String {
    if let Err(why) = start_com() {
        return why;
    }
    let library = match SrClient::load() {
        Ok(_) => "SrClient.dll loads and has SRSetRestorePointW".to_string(),
        Err(why) => why,
    };
    let namespaces = WMIConnection::with_namespace_path(NAMESPACE)
        .and_then(|wmi| wmi.raw_query::<HashMap<String, Variant>>("SELECT Name FROM __NAMESPACE"));
    let wmi = match namespaces {
        Ok(rows) => format!("WMI lists {} namespaces in {NAMESPACE}", rows.len()),
        Err(why) => format!("WMI wouldn't list the namespaces in {NAMESPACE}: {why}"),
    };
    let newest = match WinPoints.newest() {
        Ok(Some(point)) => format!(
            "the newest restore point is #{} from {}",
            point.sequence, point.time
        ),
        Ok(None) => "Windows lists no restore points".to_string(),
        Err(why) => why,
    };
    format!("{library} | {wmi} | {newest}")
}

/// Starts COM the way Microsoft's sample does before `SRSetRestorePointW`. Only a process of the helper's kind may call
/// this, once, before anything else there has used COM. COM stays up until the process ends, a moment later.
fn start_com() -> Result<(), String> {
    // SAFETY: the first argument is reserved and must be null, and the second is a flag this thread hasn't been given
    // another of, since nothing has used COM here yet.
    let started = unsafe { CoInitializeEx(null(), COINIT_MULTITHREADED as u32) };
    if started < 0 {
        return Err(com_refused(parse::hex(started as u32)));
    }
    let_system_restore_call_back()
}

fn com_refused(why: impl Display) -> String {
    format!("Windows wouldn't set up what a restore point needs: {why}.")
}

/// Room for any SID.
#[repr(align(8))]
struct Sid([u8; SECURITY_MAX_SID_SIZE as usize]);

impl Sid {
    fn well_known(kind: WELL_KNOWN_SID_TYPE) -> Result<Sid, String> {
        let mut sid = Sid([0; SECURITY_MAX_SID_SIZE as usize]);
        let mut size = SECURITY_MAX_SID_SIZE;
        // SAFETY: `sid` has room for the `size` bytes it's said to have, and the kinds asked for here take no domain.
        let made = unsafe { CreateWellKnownSid(kind, null_mut(), sid.pointer(), &mut size) };
        if made == 0 {
            return Err(com_refused(io::Error::last_os_error()));
        }
        Ok(sid)
    }

    fn pointer(&mut self) -> PSID {
        self.0.as_mut_ptr().cast()
    }

    /// The entry that lets this account call this process, as Microsoft's sample has it.
    fn may_call(&mut self) -> EXPLICIT_ACCESS_W {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: COM_RIGHTS_EXECUTE | COM_RIGHTS_EXECUTE_LOCAL,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                ptstrName: self.pointer().cast(),
            },
        }
    }
}

/// An ACL Windows allocated, freed when this is dropped.
struct LocalAcl(*mut ACL);

impl Drop for LocalAcl {
    fn drop(&mut self) {
        // SAFETY: `SetEntriesInAclW` says to free it with LocalFree, and this is the only owner.
        unsafe { LocalFree(self.0.cast()) };
    }
}

fn check(done: BOOL) -> Result<(), String> {
    if done == 0 {
        Err(com_refused(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

/// Microsoft's `InitializeCOMSecurity`, from "Using System Restore": NetworkService, LocalService and System have to be
/// able to call back into a process that uses `SRSetRestorePoint`. Administrators own the descriptor, its DACL lets in
/// those three, Administrators and the Self account, and the levels and flags are Microsoft's. It is built by hand
/// because, as the sample says, `CoInitializeSecurity` doesn't take the relative ones the SDDL functions return.
fn let_system_restore_call_back() -> Result<(), String> {
    let mut administrators = Sid::well_known(WinBuiltinAdministratorsSid)?;
    let mut local_service = Sid::well_known(WinLocalServiceSid)?;
    let mut network_service = Sid::well_known(WinNetworkServiceSid)?;
    let mut this_account = Sid::well_known(WinSelfSid)?;
    let mut system = Sid::well_known(WinLocalSystemSid)?;
    let entries = [
        administrators.may_call(),
        local_service.may_call(),
        network_service.may_call(),
        this_account.may_call(),
        system.may_call(),
    ];
    let mut acl: *mut ACL = null_mut();
    // SAFETY: `entries` holds as many as the count says, and the SIDs they point to are alive. There is no old ACL, and
    // `acl` is a valid place for the new one.
    let made =
        unsafe { SetEntriesInAclW(entries.len() as u32, entries.as_ptr(), null(), &mut acl) };
    if made != ERROR_SUCCESS || acl.is_null() {
        return Err(com_refused(io::Error::from_raw_os_error(made as i32)));
    }
    let acl = LocalAcl(acl);

    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let pointer: PSECURITY_DESCRIPTOR = (&raw mut descriptor).cast();
    // SAFETY: `pointer` points to a SECURITY_DESCRIPTOR, as big as it needs to be, in this frame.
    check(unsafe { InitializeSecurityDescriptor(pointer, SECURITY_DESCRIPTOR_REVISION) })?;
    // SAFETY: the descriptor is initialized, and keeps this pointer to a SID that outlives it.
    check(unsafe { SetSecurityDescriptorOwner(pointer, administrators.pointer(), 0) })?;
    // SAFETY: as above.
    check(unsafe { SetSecurityDescriptorGroup(pointer, administrators.pointer(), 0) })?;
    // SAFETY: as above, and `acl` outlives the descriptor too.
    check(unsafe { SetSecurityDescriptorDacl(pointer, 1, acl.0, 0) })?;

    // SAFETY: the descriptor is in the absolute form this takes, with an owner, a group and no system ACL. -1 asks COM
    // to choose the authentication services, which takes the next argument being null. The other pointers are reserved
    // or unused and must be null. COM copies what it keeps: Microsoft's sample frees all of this straight after.
    let set = unsafe {
        CoInitializeSecurity(
            pointer,
            -1,
            null(),
            null(),
            RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
            RPC_C_IMP_LEVEL_IDENTIFY,
            null(),
            (EOAC_DISABLE_AAA | EOAC_NO_CUSTOM_MARSHAL) as u32,
            null(),
        )
    };
    if set < 0 {
        return Err(com_refused(parse::hex(set as u32)));
    }
    Ok(())
}

/// `SRSetRestorePointW`, as its header declares it.
type SetRestorePoint =
    unsafe extern "system" fn(*const RESTOREPOINTINFOW, *mut STATEMGRSTATUS) -> BOOL;

/// A library Windows loaded, freed when this is dropped.
struct Library(HMODULE);

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: it was loaded by `LoadLibraryExW` and is freed once.
        unsafe { FreeLibrary(self.0) };
    }
}

/// `SrClient.dll`, loaded at run time from System32 as Microsoft says to. `windows-sys`'s own binding of the function
/// points at `sfc.dll`, so only its types are used.
struct SrClient {
    /// Holds `set_point`'s code in the process.
    _library: Library,
    set_point: SetRestorePoint,
}

impl SrClient {
    fn load() -> Result<SrClient, String> {
        let name = parse::wide("SrClient.dll");
        // SAFETY: `name` is a terminated wide string, and the second argument is reserved and must be null.
        let module =
            unsafe { LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if module.is_null() {
            let why = io::Error::last_os_error();
            return Err(format!("This PC has no System Restore: {why}"));
        }
        let library = Library(module);
        // SAFETY: `module` is loaded, and the name is a terminated ASCII string.
        let found = unsafe { GetProcAddress(module, c"SRSetRestorePointW".as_ptr().cast()) };
        let Some(found) = found else {
            let why = io::Error::last_os_error();
            return Err(format!("This PC has no System Restore: {why}"));
        };
        // SAFETY: the function by that name has exactly this signature, and `library` keeps it loaded as long as this
        // struct holds the pointer.
        let set_point =
            unsafe { transmute::<unsafe extern "system" fn() -> isize, SetRestorePoint>(found) };
        Ok(SrClient {
            _library: library,
            set_point,
        })
    }

    /// Calls the function: the sequence number on success, or the status Windows left on failure.
    fn set(&self, spec: &RESTOREPOINTINFOW) -> Result<i64, u32> {
        let mut status = STATEMGRSTATUS::default();
        // SAFETY: both pointers are valid for the call, the first to a whole structure and the second to one to fill
        // in, and `self` keeps the function loaded.
        let succeeded = unsafe { (self.set_point)(spec, &mut status) } != 0;
        if succeeded {
            Ok(status.llSequenceNumber)
        } else {
            Err(status.nStatus)
        }
    }
}

/// Restore points on this PC, for a process that has been through `start_com`.
struct WinPoints;

impl RestorePoints for WinPoints {
    fn newest(&self) -> Result<Option<Point>, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Listed {
            sequence_number: u32,
            creation_time: Option<String>,
        }
        let listed = WMIConnection::with_namespace_path(NAMESPACE)
            .and_then(|wmi| wmi.raw_query::<Listed>(LIST_POINTS))
            .map_err(|why| wmi_failure("list its restore points", &why))?;
        Ok(newest_of(listed.into_iter().map(|point| Point {
            sequence: point.sequence_number,
            time: parse::local_time(point.creation_time.as_deref().unwrap_or_default()),
        })))
    }

    /// Opens a point with `BEGIN_SYSTEM_CHANGE`, and ends it with `END_SYSTEM_CHANGE` only if a new one opened: the
    /// number answered when Windows skips making one belongs to an earlier point, which isn't this call's to end. The
    /// point exists from the first call, so the second one failing changes nothing, and its result isn't looked at. It
    /// reads the list for itself because `make_point`'s read isn't passed down to it.
    fn create(&self, description: &str) -> Result<u32, CreateError> {
        let client = SrClient::load().map_err(CreateError::Other)?;
        let before = self.newest();
        let mut spec = RESTOREPOINTINFOW {
            dwEventType: BEGIN_SYSTEM_CHANGE,
            dwRestorePtType: MODIFY_SETTINGS,
            llSequenceNumber: 0,
            szDescription: parse::restore_description(description),
        };
        let sequence = client.set(&spec).map_err(create_error)?;
        if opened_a_new_point(&before, sequence) {
            spec.dwEventType = END_SYSTEM_CHANGE;
            spec.llSequenceNumber = sequence;
            let _ = client.set(&spec);
        }
        u32::try_from(sequence).map_err(|_| {
            CreateError::Other(format!(
                "Windows answered with a restore point number no point can have: {sequence}."
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::first_line_of;
    use std::process::Command;
    use std::time::Instant;

    #[test]
    fn the_windows_drive_is_the_one_the_windows_folder_is_on() {
        let drive = system_drive().unwrap();
        assert_eq!(drive.len(), 3, "{drive}");
        assert!(drive.ends_with(r":\"), "{drive}");
        let root = std::env::var("SystemRoot").unwrap();
        assert!(root
            .to_ascii_lowercase()
            .starts_with(&drive.to_ascii_lowercase()));
    }

    #[test]
    fn the_windows_drive_has_a_volume_with_a_guid() {
        let volume = system_volume().unwrap();
        assert!(volume.starts_with(r"\\?\Volume{"), "{volume}");
        assert!(volume.ends_with(r"}\"), "{volume}");
        assert!(parse::volume_guid(&volume).is_some(), "{volume}");
    }

    #[test]
    fn reading_system_protection_answers_the_same_twice() {
        assert_eq!(protection(), protection());
    }

    #[test]
    fn the_list_of_restore_points_answers_or_says_why() {
        // A read, and a Windows with no System Restore answers with a sentence.
        let listed = on_a_thread_of_its_own(|| WinPoints.newest());
        match listed.expect("WMI should answer") {
            Ok(_) => {}
            Err(why) => assert!(why.ends_with('.'), "{why}"),
        }
    }

    #[test]
    fn system_restores_library_loads_or_says_why() {
        if let Err(why) = SrClient::load() {
            assert!(why.starts_with("This PC has no System Restore: "), "{why}");
        }
    }

    #[test]
    fn work_on_its_own_thread_that_panics_is_no_answer() {
        let answer: Option<u32> = on_a_thread_of_its_own(|| panic!("for this test"));
        assert_eq!(answer, None);
    }

    #[test]
    fn a_windows_program_answers_through_the_pipe() {
        let mut echo = Command::new("cmd");
        echo.args(["/C", "echo one line"]);
        let line = first_line_of(echo, Duration::from_secs(60)).unwrap();
        assert_eq!(line.as_deref().map(str::trim), Some("one line"));
    }

    #[test]
    fn a_windows_program_past_its_time_is_given_up_on() {
        let mut stuck = Command::new("powershell");
        stuck.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 120"]);
        let started = Instant::now();
        let line = first_line_of(stuck, Duration::from_secs(10)).unwrap();
        assert_eq!(line, None);
        assert!(started.elapsed() < Duration::from_secs(60));
    }
}
