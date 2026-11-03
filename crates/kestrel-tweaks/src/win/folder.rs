//! The journal's folder on Windows: where it goes, who owns it, and locking it down. See `crate::folder` for why.
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use kestrel_probe::account::account;
use windows_sys::core::{BOOL, PWSTR};
use windows_sys::Win32::Foundation::{LocalFree, ERROR_ALREADY_EXISTS, ERROR_SUCCESS};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    GetNamedSecurityInfoW, SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, ACL, DACL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

use crate::folder::{not_trusted, owner_is_trusted, LOCKED_DOWN};
use crate::journal::Folder;
use crate::parse;

const NOWHERE: &str =
    "Windows didn't say where the ProgramData folder is, so Kestrel has nowhere to keep its journal.";

/// A folder only administrators can write to, as `Folder` asks for.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdminFolder;

impl Folder for AdminFolder {
    /// A folder that isn't there is fine. Anything else at that path has to be a real folder with a trusted owner. A
    /// link is refused along with a file: Windows would read the owner of whatever it points to, so a link a standard
    /// user made to an administrators' folder would pass, and Kestrel would then lock that folder down and write in it.
    fn check(&self, dir: &Path) -> io::Result<()> {
        if dir.as_os_str().is_empty() {
            return Err(io::Error::other(NOWHERE));
        }
        let found = match std::fs::symlink_metadata(dir) {
            Ok(found) => found,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let me = account().this_process.unwrap_or_default();
        // `is_dir` is false for a link, even one to a folder.
        if found.is_dir() && owner_is_trusted(&owner_of(dir)?, &me) {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            not_trusted(dir),
        ))
    }

    /// A new folder is locked down from the moment it exists, so nobody can slip a file into it first. Whoever made it
    /// in the moment since the first check is judged by a second one, and the folder is then given the owner and access
    /// whether it is new or not.
    fn prepare(&self, dir: &Path) -> io::Result<()> {
        self.check(dir)?;
        let locked = LockedDown::new()?;
        let path = wide_path(dir);
        locked.create(&path)?;
        self.check(dir)?;
        locked.apply(&path)
    }
}

/// `FOLDERID_ProgramData`, as Windows names it. None when Windows won't say.
pub(crate) fn program_data() -> Option<PathBuf> {
    let mut path: PWSTR = null_mut();
    // SAFETY: the folder's id is a constant, 0 asks for nothing special, a null token means this process's own, and
    // `path` is a valid place for the string Windows allocates.
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, null_mut(), &mut path) };
    let found = (result >= 0 && !path.is_null()).then(|| {
        // SAFETY: on success Windows wrote a NUL-terminated string at `path`, which is freed only below.
        PathBuf::from(OsString::from_wide(unsafe { units(path) }))
    });
    // SAFETY: Microsoft says to free it with CoTaskMemFree whether or not the call worked. A null pointer is ignored,
    // and the string isn't used again.
    unsafe { CoTaskMemFree(path.cast_const().cast()) };
    found
}

/// The units of a NUL-terminated wide string.
///
/// # Safety
/// `text` must point to a NUL-terminated string that stays valid for as long as the answer is used.
unsafe fn units<'a>(text: *const u16) -> &'a [u16] {
    let mut length = 0;
    // SAFETY: the string is NUL-terminated, so every unit up to that NUL can be read.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the `length` units before the NUL are readable and initialised.
    unsafe { std::slice::from_raw_parts(text, length) }
}

fn wide_path(dir: &Path) -> Vec<u16> {
    dir.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Memory Windows allocated and said to free with `LocalFree`, freed when dropped. Null is fine.
struct LocalMemory(*mut core::ffi::c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a call that says to free it with LocalFree, and is freed only here. LocalFree
        // ignores a null one.
        unsafe { LocalFree(self.0) };
    }
}

/// A SID as text, `S-1-5-…`. None for no SID.
fn sid_text(sid: PSID) -> Option<String> {
    if sid.is_null() {
        return None;
    }
    let mut text: PWSTR = null_mut();
    // SAFETY: `sid` points to a valid SID and `text` is a valid place for the string Windows allocates.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 || text.is_null() {
        return None;
    }
    let _text = LocalMemory(text.cast());
    // SAFETY: Windows wrote a NUL-terminated string at `text`, which `_text` keeps until this returns.
    Some(String::from_utf16_lossy(unsafe { units(text) }))
}

/// Who owns a file or folder, as a SID. An owner that can't be read or named is an error, so it is never trusted.
fn owner_of(dir: &Path) -> io::Result<String> {
    let path = wide_path(dir);
    let mut owner: PSID = null_mut();
    let mut description: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `path` is NUL-terminated; `owner` and `description` are valid places for what is asked for; what isn't
    // asked for may be null.
    let code = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut description,
        )
    };
    // `owner` points into this, so it goes last, however this ends.
    let _description = LocalMemory(description);
    if code != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    sid_text(owner).ok_or_else(|| {
        io::Error::other(format!("Windows wouldn't say who owns {}.", dir.display()))
    })
}

/// `LOCKED_DOWN` as Windows' own structure, and what is applied from it. `owner` and `dacl` point into `description`.
struct LockedDown {
    description: LocalMemory,
    owner: PSID,
    dacl: *mut ACL,
}

impl LockedDown {
    fn new() -> io::Result<Self> {
        let sddl = parse::wide(LOCKED_DOWN);
        let mut description: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `sddl` is NUL-terminated and `description` is a valid place for what Windows allocates. The size
        // isn't wanted, and may be null.
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut description,
                null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        let description = LocalMemory(description);
        let mut owner: PSID = null_mut();
        let mut defaulted: BOOL = 0;
        // SAFETY: `description` holds a valid security descriptor; the others are valid places for the answers.
        if unsafe { GetSecurityDescriptorOwner(description.0, &mut owner, &mut defaulted) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut present: BOOL = 0;
        let mut dacl: *mut ACL = null_mut();
        // SAFETY: as above.
        let read = unsafe {
            GetSecurityDescriptorDacl(description.0, &mut present, &mut dacl, &mut defaulted)
        };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        // A null DACL would let everyone in, so nothing short of the real ones ever goes on the folder.
        if owner.is_null() || present == 0 || dacl.is_null() {
            return Err(io::Error::other(
                "Windows didn't read the owner and access for Kestrel's folder.",
            ));
        }
        Ok(Self {
            description,
            owner,
            dacl,
        })
    }

    /// Makes the folder with this owner and access. One that is already there is left for `apply`.
    fn create(&self, path: &[u16]) -> io::Result<()> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.description.0,
            bInheritHandle: 0,
        };
        // SAFETY: `path` is NUL-terminated and `attributes` is valid, as is the security descriptor it points to.
        if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32) {
            return Ok(());
        }
        Err(error)
    }

    /// Gives a folder this owner and access. The protected flag cuts it off from what the folder above passes down.
    fn apply(&self, path: &[u16]) -> io::Result<()> {
        // SAFETY: `path` is NUL-terminated; `owner` and `dacl` point into `description`, which outlives the call; the
        // group and SACL aren't being set, and may be null.
        let code = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                self.owner,
                null_mut(),
                self.dacl,
                null(),
            )
        };
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// The Administrators group, by SID.
    const ADMINISTRATORS: &str = "S-1-5-32-544";

    #[test]
    fn windows_names_the_programdata_folder() {
        let found = program_data().expect("Windows should name ProgramData");
        assert!(found.is_absolute() && found.is_dir(), "{}", found.display());
    }

    #[test]
    fn the_locked_down_description_is_what_it_says() {
        let locked = LockedDown::new().unwrap();
        assert_eq!(sid_text(locked.owner).as_deref(), Some(ADMINISTRATORS));
        // SAFETY: `dacl` was checked for null and points into `description`, which `locked` keeps.
        let aces = unsafe { (*locked.dacl).AceCount };
        assert_eq!(aces, 2);
    }

    #[test]
    fn a_folder_that_is_not_there_passes_the_check() {
        let parent = tempfile::tempdir().unwrap();
        AdminFolder.check(&parent.path().join("missing")).unwrap();
    }

    #[test]
    fn no_path_at_all_is_refused_and_says_why() {
        let refused = AdminFolder.check(Path::new("")).unwrap_err();
        assert_eq!(refused.to_string(), NOWHERE);
    }

    #[test]
    fn a_folder_this_account_made_passes_the_check() {
        let dir = tempfile::tempdir().unwrap();
        AdminFolder.check(dir.path()).unwrap();
    }

    #[test]
    fn the_owner_is_read_as_a_sid() {
        let dir = tempfile::tempdir().unwrap();
        let owner = owner_of(dir.path()).unwrap();
        assert!(owner.starts_with("S-1-"), "{owner}");
    }

    #[test]
    fn a_file_in_the_folders_place_is_refused() {
        let parent = tempfile::tempdir().unwrap();
        let file = parent.path().join("Kestrel");
        std::fs::write(&file, "").unwrap();
        let refused = AdminFolder.check(&file).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_link_in_the_folders_place_is_refused_whoever_owns_what_it_points_to() {
        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("target");
        let link = parent.path().join("Kestrel");
        std::fs::create_dir(&target).unwrap();
        // A directory junction needs no special right, which is what makes it worth refusing.
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        AdminFolder.check(&target).unwrap();
        let refused = AdminFolder.check(&link).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(refused.to_string(), not_trusted(&link));
    }
}
