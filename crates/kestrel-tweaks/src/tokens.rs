//! Changing one token of a `Name=Value;` string without disturbing the others. Windows keeps a person's Auto HDR, VRR
//! and windowed-game settings together in one string value, and a game's graphics-card choice is another token in the
//! game's own value, so Kestrel reads and writes only its own token and everything else comes out as it went in.
//!
//! The merge rule: split on `;`, split each part on its first `=`, compare names ignoring case, keep every other part
//! exactly, and write each part followed by `;`. Empty parts (a stray `;`) are dropped. A part with no `=` is not a
//! `Name=Value` token, so it is never taken for the one asked for; it is kept as it came. Names and values must not
//! contain `;`, and a name must not contain `=`.

/// One `;`-separated part of the string.
enum Part<'a> {
    /// `Name=Value`, split on the first `=`, so a value may contain more of them.
    Pair(&'a str, &'a str),
    /// Anything with no `=` in it.
    Bare(&'a str),
}

impl Part<'_> {
    /// Writes the part the way it is kept: as it came, followed by `;`.
    fn write(&self, out: &mut String) {
        match self {
            Part::Pair(name, value) => {
                out.push_str(name);
                out.push('=');
                out.push_str(value);
            }
            Part::Bare(text) => out.push_str(text),
        }
        out.push(';');
    }
}

fn parts(string: &str) -> impl Iterator<Item = Part<'_>> {
    string
        .split(';')
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((name, value)) => Part::Pair(name, value),
            None => Part::Bare(part),
        })
}

/// True for a token called `name`, in any case.
fn is_named(found: &str, name: &str) -> bool {
    found.eq_ignore_ascii_case(name)
}

/// The value of the token `name`; the last one wins when the string has it more than once. None when it has none.
pub fn get(string: &str, name: &str) -> Option<String> {
    parts(string)
        .filter_map(|part| match part {
            Part::Pair(found, value) if is_named(found, name) => Some(value),
            _ => None,
        })
        .last()
        .map(str::to_string)
}

/// The string with the token `name` set to `value`. A token already there is changed where it stands and keeps the
/// spelling it had, and further copies of it are dropped; otherwise the token goes on the end.
pub fn set(string: &str, name: &str, value: &str) -> String {
    let mut out = String::new();
    let mut placed = false;
    for part in parts(string) {
        match part {
            Part::Pair(found, _) if is_named(found, name) => {
                if !placed {
                    Part::Pair(found, value).write(&mut out);
                    placed = true;
                }
            }
            other => other.write(&mut out),
        }
    }
    if !placed {
        Part::Pair(name, value).write(&mut out);
    }
    out
}

/// The string without the token `name`, however many times it is there. An empty string when it was the only one.
pub fn remove(string: &str, name: &str) -> String {
    let mut out = String::new();
    for part in parts(string) {
        match part {
            Part::Pair(found, _) if is_named(found, name) => {}
            other => other.write(&mut out),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: &str = "SwapEffectUpgradeEnable";

    #[test]
    fn set_on_empty() {
        assert_eq!(set("", N, "1"), "SwapEffectUpgradeEnable=1;");
    }

    #[test]
    fn set_keeps_other_tokens_and_their_order() {
        assert_eq!(
            set("VRROptimizeEnable=0;AutoHDREnable=2097;", N, "1"),
            "VRROptimizeEnable=0;AutoHDREnable=2097;SwapEffectUpgradeEnable=1;"
        );
    }

    #[test]
    fn set_replaces_in_place_keeping_the_spelling() {
        assert_eq!(
            set("a=1;swapeffectupgradeenable=0;b=2;", N, "1"),
            "a=1;swapeffectupgradeenable=1;b=2;"
        );
    }

    #[test]
    fn duplicates_collapse_last_wins() {
        assert_eq!(
            get("SwapEffectUpgradeEnable=0;swapeffectupgradeenable=1;", N).as_deref(),
            Some("1")
        );
        assert_eq!(
            set(
                "SwapEffectUpgradeEnable=0;swapeffectupgradeenable=0;x=1",
                N,
                "1"
            ),
            "SwapEffectUpgradeEnable=1;x=1;"
        );
    }

    #[test]
    fn odd_contents_survive() {
        assert_eq!(
            set("Foo;;A=b=c;Bare", N, "1"),
            "Foo;A=b=c;Bare;SwapEffectUpgradeEnable=1;"
        );
        assert_eq!(get("A=b=c;", "A").as_deref(), Some("b=c"));
        assert_eq!(
            remove("Foo;A=b=c;SwapEffectUpgradeEnable=1;", N),
            "Foo;A=b=c;"
        );
    }

    #[test]
    fn remove_the_only_token_leaves_an_empty_string() {
        assert_eq!(remove("swapeffectupgradeenable=1;", N), "");
    }

    #[test]
    fn get_absent_is_none() {
        assert_eq!(get("A=1;", N), None);
        assert_eq!(get("", N), None);
    }

    #[test]
    fn a_name_has_to_match_whole_not_as_a_prefix_or_a_suffix() {
        let others = "XSwapEffectUpgradeEnable=1;SwapEffectUpgradeEnableX=1;";
        assert_eq!(get(others, N), None);
        // Neither is touched by setting or removing the real one.
        assert_eq!(
            set(others, N, "1"),
            format!("{others}SwapEffectUpgradeEnable=1;")
        );
        assert_eq!(remove(others, N), others);
    }

    #[test]
    fn a_token_with_no_equals_sign_is_never_the_one_asked_for() {
        // It isn't a `Name=Value` token, so it is kept as it came and not taken for the setting.
        let bare = "SwapEffectUpgradeEnable;";
        assert_eq!(get(bare, N), None);
        assert_eq!(
            set(bare, N, "1"),
            "SwapEffectUpgradeEnable;SwapEffectUpgradeEnable=1;"
        );
        assert_eq!(
            remove("SwapEffectUpgradeEnable;SwapEffectUpgradeEnable=1;", N),
            bare
        );
    }

    #[test]
    fn an_empty_value_is_still_a_value() {
        assert_eq!(get("SwapEffectUpgradeEnable=;", N).as_deref(), Some(""));
        assert_eq!(
            set("SwapEffectUpgradeEnable=;", N, "1"),
            "SwapEffectUpgradeEnable=1;"
        );
    }

    #[test]
    fn unknown_tokens_keep_their_own_spelling_and_values() {
        let before = "VRROptimizeEnable=1;autohdrenable=2097;Odd=a=b;";
        assert_eq!(
            remove(&set(before, N, "1"), N),
            before,
            "undoing Kestrel's token leaves the rest exactly as it was"
        );
    }

    #[test]
    fn setting_then_removing_gives_back_every_other_token() {
        // Whatever else the string holds, taking Kestrel's token out again leaves what removing it from the original
        // would: nothing but Kestrel's token is ever touched.
        for before in [
            "",
            ";",
            ";;;",
            "Foo",
            "Foo;;A=b=c;Bare",
            "=;==;=x",
            "a=1;A=2;b=3",
            "SwapEffectUpgradeEnable=0;x=1;swapeffectupgradeenable=5",
            "Sp ace=1 ; tab\t=2",
        ] {
            assert_eq!(
                remove(&set(before, N, "1"), N),
                remove(before, N),
                "{before:?}"
            );
            assert_eq!(
                get(&set(before, N, "1"), N).as_deref(),
                Some("1"),
                "{before:?}"
            );
            assert_eq!(get(&remove(before, N), N), None, "{before:?}");
        }
    }
}
