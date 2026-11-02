//! The fixed facts about each tweak: what it is called, how much it can matter in game, and when it takes effect.
//! Per-tweak text lives here in Rust, as Findings' text does; the page's own labels live in `strings.ts`.
use kestrel_core::tweak_id::TweakId;
use serde::{Deserialize, Serialize};

/// How much a tweak can matter in game, as the Tweaks tab tags it. It is never a promised number: the copy says
/// "Measure it on your PC".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Impact {
    High,
    Medium,
    Low,
    /// Only matters in some set-ups, such as a game running in a window.
    Situational,
    /// Changes nothing in the game itself.
    NoneInGame,
}

/// When a change takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Timing {
    Now,
    /// The game reads it as it starts.
    NextLaunch,
    NextSignIn,
}

/// A tweak's title and tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub id: TweakId,
    pub title: &'static str,
    pub impact: Impact,
    pub timing: Timing,
}

/// The title and tags of a tweak, from the spec's Essential table. System Protection is not in that table; it is the
/// safety net rather than a tweak, so it changes nothing in the game.
pub fn info(id: TweakId) -> Info {
    let (title, impact, timing) = match id {
        TweakId::PowerPlan => ("Kestrel power plan", Impact::Low, Timing::Now),
        TweakId::GameMode => ("Game Mode", Impact::Medium, Timing::Now),
        TweakId::BackgroundRecording => ("Background recording off", Impact::Medium, Timing::Now),
        TweakId::WindowedOptimizations => (
            "Optimizations for windowed games",
            Impact::Situational,
            Timing::NextLaunch,
        ),
        TweakId::GpuPreference => ("GPU preference", Impact::High, Timing::NextLaunch),
        TweakId::PointerPrecision => (
            "Enhance pointer precision off",
            Impact::NoneInGame,
            Timing::Now,
        ),
        TweakId::SystemProtection => ("System Protection", Impact::NoneInGame, Timing::Now),
    };
    Info {
        id,
        title,
        impact,
        timing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tweak_has_the_title_and_tags_the_spec_gives_it_in_display_order() {
        let table = [
            (
                TweakId::PowerPlan,
                "Kestrel power plan",
                Impact::Low,
                Timing::Now,
            ),
            (TweakId::GameMode, "Game Mode", Impact::Medium, Timing::Now),
            (
                TweakId::BackgroundRecording,
                "Background recording off",
                Impact::Medium,
                Timing::Now,
            ),
            (
                TweakId::WindowedOptimizations,
                "Optimizations for windowed games",
                Impact::Situational,
                Timing::NextLaunch,
            ),
            (
                TweakId::GpuPreference,
                "GPU preference",
                Impact::High,
                Timing::NextLaunch,
            ),
            (
                TweakId::PointerPrecision,
                "Enhance pointer precision off",
                Impact::NoneInGame,
                Timing::Now,
            ),
            // The spec's table has no row for System Protection; it changes nothing in the game.
            (
                TweakId::SystemProtection,
                "System Protection",
                Impact::NoneInGame,
                Timing::Now,
            ),
        ];
        // The table covers every tweak, the Essential rows in their display order and then System Protection.
        let mut every = TweakId::ESSENTIAL.to_vec();
        every.push(TweakId::SystemProtection);
        assert_eq!(table.map(|row| row.0).to_vec(), every);

        for (id, title, impact, timing) in table {
            assert_eq!(
                info(id),
                Info {
                    id,
                    title,
                    impact,
                    timing
                },
                "{id:?}"
            );
        }
    }

    #[test]
    fn titles_are_not_shared_between_tweaks() {
        let mut every = TweakId::ESSENTIAL.to_vec();
        every.push(TweakId::SystemProtection);
        let titles: HashSet<&str> = every.iter().map(|id| info(*id).title).collect();
        assert_eq!(titles.len(), every.len());
        assert!(titles.iter().all(|title| !title.trim().is_empty()));
    }

    #[test]
    fn tags_are_spelled_in_kebab_case_the_way_the_page_reads_them() {
        for (impact, text) in [
            (Impact::High, "high"),
            (Impact::Medium, "medium"),
            (Impact::Low, "low"),
            (Impact::Situational, "situational"),
            (Impact::NoneInGame, "none-in-game"),
        ] {
            let json = serde_json::to_string(&impact).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<Impact>(&json).unwrap(), impact);
        }
        for (timing, text) in [
            (Timing::Now, "now"),
            (Timing::NextLaunch, "next-launch"),
            (Timing::NextSignIn, "next-sign-in"),
        ] {
            let json = serde_json::to_string(&timing).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<Timing>(&json).unwrap(), timing);
        }
    }
}
