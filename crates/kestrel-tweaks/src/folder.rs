//! Where the journal lives and who may own that folder. An elevated Kestrel must not act on a file any user can edit,
//! so on Windows the folder belongs to the Administrators group, only Administrators and SYSTEM can write to it, and a
//! folder someone else made beforehand is refused. The Windows calls are in `win::folder`; what they decide by is here,
//! so it is tested on any OS.
use std::path::{Path, PathBuf};

use crate::journal::JournalFile;

/// The Administrators group, by SID.
const ADMINISTRATORS: &str = "S-1-5-32-544";
/// The SYSTEM account, by SID.
const LOCAL_SYSTEM: &str = "S-1-5-18";

/// The owner and access the journal's folder is given: Administrators own it; Administrators and SYSTEM have full
/// control of it and of everything in it; nobody else has anything, and nothing is inherited from the folder above.
pub const LOCKED_DOWN: &str = "O:BAD:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)";

/// Whether a folder with this owner may hold the journal: Administrators, SYSTEM, or the account Kestrel itself runs
/// as. `owner` and `me` are SIDs. An owner that couldn't be named is nobody to trust.
pub fn owner_is_trusted(owner: &str, me: &str) -> bool {
    owner == ADMINISTRATORS || owner == LOCAL_SYSTEM || (!owner.is_empty() && owner == me)
}

/// What Apply says when the folder is refused.
pub fn not_trusted(dir: &Path) -> String {
    format!(
        "{} belongs to another account. Delete it, then try again.",
        dir.display()
    )
}

/// The journal's folder: `Kestrel` under ProgramData, as Windows names that folder and not as an environment variable
/// says, or off Windows a folder under the temporary one. None when Windows wouldn't say.
pub fn journal_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::program_data().map(|data| data.join("Kestrel"))
    }
    #[cfg(not(windows))]
    {
        Some(std::env::temp_dir().join("kestrel-dev"))
    }
}

/// This PC's journal. It touches nothing yet. When Windows wouldn't say where the folder goes, the journal has no path,
/// which `AdminFolder` refuses and says why, so Apply stops before any change.
pub fn journal_file() -> JournalFile {
    #[cfg(windows)]
    let folder = crate::win::AdminFolder;
    #[cfg(not(windows))]
    let folder = crate::journal::PlainFolder;
    JournalFile::new(journal_dir().unwrap_or_default(), Box::new(folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_admins_system_or_this_process_own_the_folder() {
        assert!(owner_is_trusted("S-1-5-32-544", "S-1-5-21-1-1001"));
        assert!(owner_is_trusted("S-1-5-18", "S-1-5-21-1-1001"));
        assert!(owner_is_trusted("S-1-5-21-1-1001", "S-1-5-21-1-1001"));
        assert!(!owner_is_trusted("S-1-5-21-1-1002", "S-1-5-21-1-1001"));
        assert!(!owner_is_trusted("", "S-1-5-21-1-1001"));
    }

    #[test]
    fn an_owner_nobody_could_name_is_not_trusted_even_if_kestrel_cannot_name_itself() {
        assert!(!owner_is_trusted("", ""));
        assert!(owner_is_trusted("S-1-5-32-544", ""));
        assert!(!owner_is_trusted("S-1-5-21-1-1002", ""));
    }

    #[test]
    fn a_refused_folder_says_what_to_do_about_it() {
        assert_eq!(
            not_trusted(Path::new(r"C:\ProgramData\Kestrel")),
            r"C:\ProgramData\Kestrel belongs to another account. Delete it, then try again."
        );
    }

    #[test]
    fn the_folder_is_locked_down_to_administrators_and_system() {
        assert_eq!(LOCKED_DOWN, "O:BAD:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)");
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_journal_stays_in_kestrels_folder_under_programdata() {
        let dir = journal_dir().expect("Windows should name ProgramData");
        assert!(dir.ends_with("Kestrel"), "{}", dir.display());
        assert!(dir.parent().is_some_and(Path::is_dir), "{}", dir.display());
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_journal_stays_under_the_temporary_folder() {
        assert_eq!(
            journal_dir(),
            Some(std::env::temp_dir().join("kestrel-dev"))
        );
    }
}
