//! Who is signed in at this PC, and who Kestrel runs as. They are the same account unless a standard user let Kestrel
//! run by typing an administrator's password: Kestrel then runs as that administrator, and its own
//! `HKEY_CURRENT_USER` is the administrator's, not that of the person at the screen. Naming both lets whatever reads or
//! changes per-user settings (Game Mode, the mouse, graphics preferences) go to the account actually using the PC.
//! Read-only: this only asks Windows who owns what, and changes nothing.
use std::sync::OnceLock;

/// Off Windows there are no accounts to name; development and tests get this one SID for both.
#[cfg(not(windows))]
const DEVELOPMENT_SID: &str = "S-1-5-21-1-2-3-1001";

/// Two accounts, each by its SID as text (`S-1-5-21-…`); None where Windows wouldn't say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account at the screen: the owner of the desktop's shell, whose Game Mode and other settings the person
    /// sees.
    pub signed_in: Option<String>,
    /// The account Kestrel's own process runs as.
    pub this_process: Option<String>,
}

impl Account {
    /// True only when both accounts are known and are not the same one. An account that couldn't be named is no
    /// proof of a mismatch.
    pub fn differs(&self) -> bool {
        matches!((&self.signed_in, &self.this_process), (Some(shell), Some(this)) if shell != this)
    }
}

/// The signed-in account and the account this process runs as. Read-only.
pub fn account() -> Account {
    // Who is at the screen can't change while Kestrel runs, so a known answer is kept for good. An unknown one (the
    // shell isn't up yet, say) is asked for again next time.
    static SIGNED_IN: OnceLock<String> = OnceLock::new();
    Account {
        signed_in: cached(&SIGNED_IN, signed_in_now),
        this_process: this_process_now(),
    }
}

/// What `cache` already keeps, else what `ask` finds, which is kept once it is known. An unknown answer is never kept.
fn cached(cache: &OnceLock<String>, ask: impl FnOnce() -> Option<String>) -> Option<String> {
    if let Some(kept) = cache.get() {
        return Some(kept.clone());
    }
    let found = ask()?;
    // If another thread kept an answer meanwhile, that first one stands.
    Some(cache.get_or_init(|| found).clone())
}

/// The owner of the desktop's shell, else the user signed in to Kestrel's session (see `win::account`).
fn signed_in_now() -> Option<String> {
    #[cfg(windows)]
    {
        crate::win::account::signed_in()
    }
    #[cfg(not(windows))]
    {
        Some(DEVELOPMENT_SID.to_string())
    }
}

/// The account of this process's token.
fn this_process_now() -> Option<String> {
    #[cfg(windows)]
    {
        crate::win::account::this_process()
    }
    #[cfg(not(windows))]
    {
        Some(DEVELOPMENT_SID.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differs_only_when_both_are_known_and_unequal() {
        let a = |s: Option<&str>, p: Option<&str>| Account {
            signed_in: s.map(Into::into),
            this_process: p.map(Into::into),
        };
        assert!(a(Some("S-1-5-21-1-1001"), Some("S-1-5-21-1-1002")).differs());
        assert!(!a(Some("S-1-5-21-1-1001"), Some("S-1-5-21-1-1001")).differs());
        assert!(!a(None, Some("S-1-5-21-1-1002")).differs());
        assert!(!a(Some("S-1-5-21-1-1001"), None).differs());
        assert!(!a(None, None).differs());
    }

    #[test]
    fn a_known_answer_is_kept_and_not_asked_for_again() {
        let cache = OnceLock::new();
        assert_eq!(
            cached(&cache, || Some("S-1-5-21-1-1001".into())).as_deref(),
            Some("S-1-5-21-1-1001")
        );
        // Asking again would give another account, and the first answer must stand.
        assert_eq!(
            cached(&cache, || Some("S-1-5-21-1-1002".into())).as_deref(),
            Some("S-1-5-21-1-1001")
        );
    }

    #[test]
    fn an_unknown_answer_is_not_kept_so_the_next_call_asks_again() {
        let cache = OnceLock::new();
        assert_eq!(cached(&cache, || None), None);
        assert_eq!(cached(&cache, || None), None);
        // Windows may answer later (the shell wasn't up yet), and that answer is the one kept.
        assert_eq!(
            cached(&cache, || Some("S-1-5-21-1-1001".into())).as_deref(),
            Some("S-1-5-21-1-1001")
        );
        assert_eq!(cached(&cache, || None).as_deref(), Some("S-1-5-21-1-1001"));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_both_accounts_are_the_same_fixed_one() {
        let who = account();
        assert_eq!(who.signed_in.as_deref(), Some("S-1-5-21-1-2-3-1001"));
        assert_eq!(who.this_process.as_deref(), Some("S-1-5-21-1-2-3-1001"));
        assert!(!who.differs());
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_this_process_is_always_named() {
        // The shell can be missing (a build runner has no desktop), but a process can always read its own token.
        assert!(account().this_process.is_some());
    }
}
