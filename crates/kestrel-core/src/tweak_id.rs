//! The tweaks Kestrel can apply, by the one name Health, the journal and the Tweaks tab all use.
use serde::{Deserialize, Serialize};

/// A tweak. Serialized as its `as_str` name, which the interface's `TweakId` type spells the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TweakId {
    PowerPlan,
    GameMode,
    BackgroundRecording,
    WindowedOptimizations,
    GpuPreference,
    PointerPrecision,
    SystemProtection,
}

impl TweakId {
    /// The Essential rows, in display order: everything but `SystemProtection`, which has a row of its own and is
    /// never part of Revert everything.
    pub const ESSENTIAL: [TweakId; 6] = [
        TweakId::PowerPlan,
        TweakId::GameMode,
        TweakId::BackgroundRecording,
        TweakId::WindowedOptimizations,
        TweakId::GpuPreference,
        TweakId::PointerPrecision,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TweakId::PowerPlan => "power-plan",
            TweakId::GameMode => "game-mode",
            TweakId::BackgroundRecording => "background-recording",
            TweakId::WindowedOptimizations => "windowed-optimizations",
            TweakId::GpuPreference => "gpu-preference",
            TweakId::PointerPrecision => "pointer-precision",
            TweakId::SystemProtection => "system-protection",
        }
    }

    /// The tweak with exactly this name, if there is one.
    pub fn parse(text: &str) -> Option<TweakId> {
        Self::ESSENTIAL
            .into_iter()
            .chain([TweakId::SystemProtection])
            .find(|id| id.as_str() == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every tweak: the Essential rows, then System Protection.
    fn all() -> Vec<TweakId> {
        let mut all = TweakId::ESSENTIAL.to_vec();
        all.push(TweakId::SystemProtection);
        all
    }

    #[test]
    fn the_essential_rows_are_every_tweak_but_system_protection_in_display_order() {
        assert_eq!(
            TweakId::ESSENTIAL,
            [
                TweakId::PowerPlan,
                TweakId::GameMode,
                TweakId::BackgroundRecording,
                TweakId::WindowedOptimizations,
                TweakId::GpuPreference,
                TweakId::PointerPrecision,
            ]
        );
        assert!(!TweakId::ESSENTIAL.contains(&TweakId::SystemProtection));
    }

    #[test]
    fn every_tweak_round_trips_through_its_name() {
        for id in all() {
            assert_eq!(TweakId::parse(id.as_str()), Some(id), "{id:?}");
        }
    }

    #[test]
    fn the_names_are_the_kebab_case_ones_the_interface_and_the_journal_use() {
        let names: Vec<&str> = all().into_iter().map(TweakId::as_str).collect();
        assert_eq!(
            names,
            [
                "power-plan",
                "game-mode",
                "background-recording",
                "windowed-optimizations",
                "gpu-preference",
                "pointer-precision",
                "system-protection",
            ]
        );
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), names.len());
        // serde spells each one the same way, so a Fix and a journal line carry the name `as_str` gives.
        for id in all() {
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.as_str()));
            assert_eq!(serde_json::from_str::<TweakId>(&json).unwrap(), id);
        }
    }

    #[test]
    fn parse_takes_the_exact_name_and_nothing_else() {
        assert_eq!(TweakId::parse("nope"), None);
        for text in [
            "",
            "Power-Plan",
            "power_plan",
            "powerplan",
            " power-plan",
            "power-plan ",
        ] {
            assert_eq!(TweakId::parse(text), None, "{text:?}");
        }
    }
}
