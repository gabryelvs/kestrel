//! Kestrel bench: turns PresentMon's output into benchmark results and keeps them. Pure Rust: the app runs the
//! capture; this crate only reads what PresentMon wrote, so it never touches a game.
pub mod csv;
pub mod metrics;
pub mod model;
pub mod profile;
pub mod settings;

pub use model::*;
pub use profile::{profile, GameProfile, Route, PROFILES, VALORANT};

/// The PresentMon release Kestrel bundles (spec "PresentMon").
pub const PRESENTMON_VERSION: &str = "2.6.0";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valorant_is_the_only_profile_and_uses_route_v1() {
        assert_eq!(PROFILES.len(), 1);
        let v = profile("valorant").expect("valorant profile");
        assert_eq!(v.process_name, "VALORANT-Win64-Shipping.exe");
        assert_eq!(
            v.route,
            Route {
                version: 1,
                capture_seconds: 90,
                switch_at_seconds: 30
            }
        );
        assert!(profile("fivem").is_none());
    }

    #[test]
    fn json_uses_camel_case_fields_and_kebab_case_tags() {
        let m = RunMetrics {
            low1_fps: 1.0,
            low01_fps: 2.0,
            pc_latency_ms: Some(3.0),
            bottleneck: Bottleneck::Gpu,
            ..Default::default()
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["low1Fps"], json!(1.0));
        assert_eq!(v["low01Fps"], json!(2.0));
        assert_eq!(v["pcLatencyMs"], json!(3.0));
        assert_eq!(v["stuttersPerMin"], json!(0.0));
        assert_eq!(v["durationS"], json!(0.0));
        assert_eq!(v["bottleneck"], json!("gpu"));
        assert_eq!(
            serde_json::to_value(Flag::BusyBackground {
                name: "chrome.exe".into()
            })
            .unwrap(),
            json!({"kind": "busy-background", "name": "chrome.exe"})
        );
        assert_eq!(
            serde_json::to_value(Flag::NotInFront).unwrap(),
            json!({"kind": "not-in-front"})
        );
        assert_eq!(
            serde_json::to_value(Verdict::RealGain).unwrap(),
            json!("real-gain")
        );
        assert_eq!(
            serde_json::to_value(MetricId::Low01Fps).unwrap(),
            json!("low01-fps")
        );
        let p = serde_json::to_value(VALORANT).unwrap();
        assert_eq!(p["processName"], json!("VALORANT-Win64-Shipping.exe"));
        assert_eq!(p["route"]["switchAtSeconds"], json!(30));
    }

    #[test]
    fn a_benchmark_round_trips_through_json() {
        let b = Benchmark {
            id: "20261012-213005".into(),
            name: STARTING_POINT.into(),
            profile_id: "valorant".into(),
            route_version: 1,
            runs: vec![RunRecord {
                flags: vec![Flag::TooShort],
                kept: true,
                ..Default::default()
            }],
            context: Context {
                game_settings: Some(ValorantSettings {
                    resolution: Some([1920, 1080]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let text = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<Benchmark>(&text).unwrap(), b);
    }
}
