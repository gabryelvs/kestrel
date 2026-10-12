//! Game profiles (decision 55). Stage 2 has one: VALORANT with Route v1.
use serde::Serialize;

/// The routine a benchmark follows. Results from different route versions are never compared.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub version: u32,
    /// How long each capture lasts.
    pub capture_seconds: u32,
    /// When the short beep tells the player to switch from the walk to the bots.
    pub switch_at_seconds: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub id: &'static str,
    pub name: &'static str,
    /// The game's process, matched by name only (Kestrel never opens it).
    pub process_name: &'static str,
    pub route: Route,
}

pub const VALORANT: GameProfile = GameProfile {
    id: "valorant",
    name: "VALORANT",
    process_name: "VALORANT-Win64-Shipping.exe",
    route: Route {
        version: 1,
        capture_seconds: 90,
        switch_at_seconds: 30,
    },
};

pub const PROFILES: &[GameProfile] = &[VALORANT];

pub fn profile(id: &str) -> Option<&'static GameProfile> {
    PROFILES.iter().find(|p| p.id == id)
}
