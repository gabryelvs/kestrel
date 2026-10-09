//! The Health Check: one finding per check, in plain words, each with a way to fix it.
use crate::gpu;
use crate::ram::{self, ChannelLayout};
use crate::snapshot::*;
use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Hardware,
    Display,
    Gpu,
    Windows,
    Storage,
    Background,
    Vanguard,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Pass,
    Problem,
    Info,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BiosTopic {
    Xmp,
    MemoryGear,
    Iommu,
    ResizableBar,
    UpdateBios,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Fix {
    BiosGuide { topic: BiosTopic },
    LaterStage { stage: u8 },
    Steps { steps: Vec<String> },
    Link { label: String, url: String },
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub category: Category,
    pub severity: Severity,
    pub status: Status,
    pub title: String,
    pub detail: String,
    pub fix: Fix,
}

/// A check's fixed identity; `pass`/`problem`/`unknown`/`info` fill in the result.
struct Check {
    id: &'static str,
    category: Category,
    severity: Severity,
}

impl Check {
    fn result(
        &self,
        status: Status,
        title: impl Into<String>,
        detail: impl Into<String>,
        fix: Fix,
    ) -> Finding {
        Finding {
            id: self.id.into(),
            category: self.category,
            severity: self.severity,
            status,
            title: title.into(),
            detail: detail.into(),
            fix,
        }
    }
    fn pass(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Pass, title, detail, Fix::Nothing)
    }
    fn problem(&self, title: impl Into<String>, detail: impl Into<String>, fix: Fix) -> Finding {
        self.result(Status::Problem, title, detail, fix)
    }
    fn unknown(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Unknown, title, detail, Fix::Nothing)
    }
}

pub(crate) fn steps(items: &[&str]) -> Fix {
    Fix::Steps {
        steps: items.iter().map(|s| s.to_string()).collect(),
    }
}

pub(crate) fn parse_date(date: &Option<String>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date.as_deref()?, "%Y-%m-%d").ok()
}

/// Runs every check. Always returns one finding per check, in a fixed order.
pub fn run_health_check(s: &SystemSnapshot, today: NaiveDate) -> Vec<Finding> {
    vec![
        ram_speed(s),
        ram_channels(s),
        bios_age(s),
        display_refresh(s),
        virtual_display(s),
        gpu_driver_age(s, today),
        valorant_gpu(s),
        on_battery(s),
    ]
}

fn ram_speed(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "ram-speed",
        category: Category::Hardware,
        severity: Severity::High,
    };
    let modules = &s.memory.modules;
    match (ram::configured_mts(modules), ram::rated_mts(modules)) {
        (Some(now), Some(rated)) if now + 50 < rated => c.problem(
            format!("RAM runs at {now} MT/s, but the kit is rated {rated}"),
            "XMP (Intel) or EXPO (AMD) is probably off in the BIOS. Turning it on is often the biggest single FPS gain. If you lowered the speed on purpose for stability, you can ignore this.",
            Fix::BiosGuide { topic: BiosTopic::Xmp },
        ),
        (Some(now), Some(rated)) => c.pass(format!("RAM runs at {now} MT/s, its rated speed"), format!("The kit is rated {rated} MT/s.")),
        _ => c.unknown(
            "Couldn't tell your RAM's rated speed",
            "Kestrel reads the rated speed from the module part number, and this kit's format isn't one it knows yet. The BIOS guide shows how to check XMP yourself.",
        ),
    }
}

fn ram_channels(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "ram-channels",
        category: Category::Hardware,
        severity: Severity::High,
    };
    let modules = &s.memory.modules;
    match ram::channel_layout(modules) {
        ChannelLayout::Dual => c.pass("RAM runs in dual channel", "Your sticks sit on separate memory channels, which doubles memory bandwidth."),
        ChannelLayout::Single if modules.len() == 1 => c.problem(
            "Only one RAM stick, so it runs in single channel",
            "Single-channel memory halves bandwidth and can cost a lot of FPS in CPU-heavy games like VALORANT.",
            steps(&["Buy a second stick that matches the first (same size and speed).", "Install it in the slot your motherboard manual recommends for two sticks (usually A2 and B2)."]),
        ),
        ChannelLayout::Single => c.problem(
            "Your RAM sticks share one memory channel",
            "They're in slots on the same channel, which halves memory bandwidth. Moving them fixes it for free.",
            steps(&["Shut down and unplug the PC.", "Move the sticks to the two slots your motherboard manual recommends for two sticks (usually A2 and B2, the second and fourth from the CPU).", "Start the PC and run the Health Check again."]),
        ),
        ChannelLayout::Unknown => c.unknown("Couldn't tell whether your RAM runs in dual channel", "Windows didn't report which slots the sticks are in."),
    }
}

fn bios_age(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "bios-age",
        category: Category::Hardware,
        severity: Severity::Low,
    };
    let cutoff = NaiveDate::from_ymd_opt(2025, 12, 1).expect("valid date");
    match parse_date(&s.board.bios_date) {
        Some(date) if date < cutoff => c.problem(
            format!("BIOS is from {date}"),
            "Riot has asked some players to install a BIOS update released after December 2025 to fix a security flaw. Updates also improve memory stability. Updating resets BIOS settings, so turn XMP back on afterwards.",
            Fix::BiosGuide { topic: BiosTopic::UpdateBios },
        ),
        Some(date) => c.pass(format!("BIOS is from {date}"), "Recent enough for Riot's December 2025 security fix."),
        None => c.unknown("Couldn't read your BIOS date", "Windows didn't report when your BIOS was released."),
    }
}

fn display_label(d: &DisplayInfo) -> String {
    if d.is_primary {
        return "Main display".into();
    }
    let digits: String = d.name.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        "A display".into()
    } else {
        format!("Display {}", digits.trim_start_matches('0'))
    }
}

fn display_refresh(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "display-refresh",
        category: Category::Display,
        severity: Severity::High,
    };
    // Win32 reports 0 or 1 Hz for "hardware default", which is not a real rate.
    let real: Vec<&DisplayInfo> = s
        .displays
        .iter()
        .filter(|d| !d.is_virtual && d.max_hz > 0 && d.current_hz > 1)
        .collect();
    if real.is_empty() {
        return c.unknown(
            "Couldn't read your monitor's refresh rate",
            "Windows didn't report any connected monitors.",
        );
    }
    match real.iter().find(|d| d.current_hz + 1 < d.max_hz) {
        Some(slow) => {
            let who = if real.len() == 1 {
                "Monitor".to_string()
            } else {
                display_label(slow)
            };
            c.problem(
                format!("{who} runs at {} Hz but supports {} Hz", slow.current_hz, slow.max_hz),
                "VALORANT can only show as many frames per second as the monitor's refresh rate. Choose the highest rate in Windows display settings.",
                steps(&["Open Settings → System → Display → Advanced display.", "Pick the display, then choose the highest refresh rate."]),
            )
        }
        None if real.len() == 1 => c.pass(
            format!(
                "Monitor runs at its top refresh rate ({} Hz)",
                real[0].max_hz
            ),
            "",
        ),
        None => c.pass("All monitors run at their top refresh rate", ""),
    }
}

fn virtual_display(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "virtual-display",
        category: Category::Display,
        severity: Severity::Low,
    };
    match s.displays.iter().find(|d| d.is_virtual) {
        Some(v) => c.problem(
            "A virtual display is active",
            format!("{} has added a virtual monitor. While it's active, Windows can render to it or limit your real monitor.", v.adapter),
            steps(&["End any remote-play session (Parsec or similar) before playing.", "If it stays on, turn the virtual display off in that app's settings."]),
        ),
        None => c.pass("No virtual displays active", ""),
    }
}

fn driver_link(vendor: GpuVendor) -> Fix {
    let (label, url) = match vendor {
        GpuVendor::Nvidia => (
            "Get the latest NVIDIA driver",
            "https://www.nvidia.com/en-us/drivers/",
        ),
        GpuVendor::Amd => (
            "Get the latest AMD driver",
            "https://www.amd.com/en/support/download/drivers.html",
        ),
        GpuVendor::Intel => (
            "Get the latest Intel driver",
            "https://www.intel.com/content/www/us/en/download-center/home.html",
        ),
        _ => {
            return steps(&["Download the latest driver from your graphics card maker's website."])
        }
    };
    Fix::Link {
        label: label.into(),
        url: url.into(),
    }
}

fn gpu_driver_age(s: &SystemSnapshot, today: NaiveDate) -> Finding {
    let c = Check {
        id: "gpu-driver-age",
        category: Category::Gpu,
        severity: Severity::Medium,
    };
    let Some(main) = gpu::main_gpu(&s.gpus) else {
        return c.unknown(
            "Couldn't find your graphics card",
            "Windows didn't report a real graphics card.",
        );
    };
    let version = gpu::display_driver_version(main);
    match parse_date(&main.driver_date) {
        Some(date) if (today - date).num_days() > 183 => c.problem(
            format!("Graphics driver is {} months old", (today - date).num_days() / 30),
            format!("{} driver {version} is from {date}. Newer drivers often fix stutter and add game optimisations.", main.name),
            driver_link(main.vendor),
        ),
        Some(date) => c.pass(format!("Graphics driver is recent ({version}, from {date})"), main.name.clone()),
        None => c.unknown("Couldn't read your graphics driver's date", format!("{} driver {version}.", main.name)),
    }
}

fn valorant_gpu(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "valorant-gpu",
        category: Category::Gpu,
        severity: Severity::High,
    };
    let Some(main) = gpu::main_gpu(&s.gpus) else {
        return c.unknown(
            "Couldn't tell which graphics chip VALORANT uses",
            "Windows didn't report a real graphics card.",
        );
    };
    if s.machine.form_factor == FormFactor::Desktop && !main.is_integrated {
        let primary = s.displays.iter().find(|d| d.is_primary && !d.is_virtual);
        let on_integrated = primary.is_some_and(|d| {
            s.gpus
                .iter()
                .any(|g| g.is_integrated && g.name == d.adapter)
        });
        if on_integrated {
            return c.problem(
                "Your monitor is plugged into the motherboard",
                format!("It's using the processor's built-in graphics instead of your {}.", main.name),
                steps(&["Shut down the PC.", "Move the monitor cable from the motherboard's video port to a port on the graphics card (lower down at the back).", "Start the PC."]),
            );
        }
        return c.pass(format!("VALORANT runs on your {}", main.name), "");
    }
    // Without a form factor, a hybrid PC could be a laptop, so the GPU choice stays unknown.
    if s.machine.form_factor == FormFactor::Unknown && gpu::is_hybrid(&s.gpus) {
        return c.unknown(
            "Couldn't tell which graphics chip VALORANT uses",
            "Windows didn't say whether this PC is a laptop or a desktop.",
        );
    }
    if !gpu::is_hybrid(&s.gpus) {
        return c.pass(format!("VALORANT runs on your {}", main.name), "");
    }
    match s.windows.valorant_gpu_preference {
        Some(2) => c.pass(format!("VALORANT is set to use your {}", main.name), ""),
        _ => c.problem(
            "VALORANT may run on the integrated graphics",
            "This PC has two graphics chips. Windows can pick the weaker one unless VALORANT is set to High performance.",
            steps(&["Open Settings → System → Display → Graphics.", "Find VALORANT (add VALORANT-Win64-Shipping.exe if it isn't listed).", "Choose Options → High performance → Save."]),
        ),
    }
}

fn on_battery(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "on-battery",
        category: Category::Gpu,
        severity: Severity::Medium,
    };
    match (s.machine.form_factor, s.machine.on_battery) {
        (FormFactor::Laptop, Some(true)) => c.problem(
            "Laptop is running on battery",
            "On battery, Windows and the graphics card cut power, which can cut FPS sharply.",
            steps(&["Plug in the charger before you play."]),
        ),
        (FormFactor::Laptop, Some(false)) => c.pass("Laptop is plugged in", ""),
        (FormFactor::Desktop, _) => c.pass("Running on mains power", ""),
        _ => c.unknown("Couldn't tell whether this PC runs on battery", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::owner_pc_fixture;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()
    }

    fn get<'a>(findings: &'a [Finding], id: &str) -> &'a Finding {
        findings
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("no finding {id}"))
    }

    fn display(
        name: &str,
        current: u32,
        max: u32,
        primary: bool,
        is_virtual: bool,
        adapter: &str,
    ) -> DisplayInfo {
        DisplayInfo {
            name: name.into(),
            adapter: adapter.into(),
            current_hz: current,
            max_hz: max,
            width: 1920,
            height: 1080,
            is_primary: primary,
            is_virtual,
        }
    }

    fn gpu(name: &str, vendor: GpuVendor, integrated: bool, date: Option<&str>) -> GpuInfo {
        GpuInfo {
            name: name.into(),
            vendor,
            driver_version: "32.0.16.1074".into(),
            driver_date: date.map(Into::into),
            vram_mb: Some(if integrated { 128 } else { 8192 }),
            is_integrated: integrated,
        }
    }

    #[test]
    fn owner_pc_part_one_results() {
        let f = run_health_check(&owner_pc_fixture(), today());
        for id in [
            "ram-speed",
            "ram-channels",
            "display-refresh",
            "virtual-display",
            "gpu-driver-age",
            "valorant-gpu",
            "on-battery",
        ] {
            assert_eq!(get(&f, id).status, Status::Pass, "{id}");
        }
        assert_eq!(get(&f, "bios-age").status, Status::Problem);
        assert_eq!(
            get(&f, "bios-age").fix,
            Fix::BiosGuide {
                topic: BiosTopic::UpdateBios
            }
        );
    }

    #[test]
    fn ram_below_rated_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        for m in &mut s.memory.modules {
            m.configured_mts = Some(2133);
        }
        let f = run_health_check(&s, today());
        let ram = get(&f, "ram-speed");
        assert_eq!(
            (ram.status, ram.severity),
            (Status::Problem, Severity::High)
        );
        assert!(
            ram.title.contains("2133") && ram.title.contains("3600"),
            "{}",
            ram.title
        );
        assert_eq!(
            ram.fix,
            Fix::BiosGuide {
                topic: BiosTopic::Xmp
            }
        );
    }

    #[test]
    fn single_stick_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.memory.modules.truncate(1);
        assert_eq!(
            get(&run_health_check(&s, today()), "ram-channels").status,
            Status::Problem
        );
    }

    #[test]
    fn empty_snapshot_gives_unknowns_not_problems() {
        let f = run_health_check(&SystemSnapshot::default(), today());
        assert!(!f.is_empty());
        for finding in &f {
            assert_ne!(
                finding.status,
                Status::Problem,
                "{} should not be a problem on an empty snapshot",
                finding.id
            );
        }
        assert_eq!(get(&f, "ram-speed").status, Status::Unknown);
        assert_eq!(get(&f, "display-refresh").status, Status::Unknown);
    }

    #[test]
    fn refresh_flags_only_the_slow_real_display() {
        let mut s = owner_pc_fixture();
        s.displays = vec![
            display(
                "\\\\.\\DISPLAY1",
                360,
                360,
                true,
                false,
                "NVIDIA GeForce RTX 3060 Ti",
            ),
            display(
                "\\\\.\\DISPLAY2",
                60,
                144,
                false,
                false,
                "NVIDIA GeForce RTX 3060 Ti",
            ),
            display(
                "\\\\.\\DISPLAY3",
                30,
                240,
                false,
                true,
                "Parsec Virtual Display Adapter",
            ),
        ];
        let refresh = get(&run_health_check(&s, today()), "display-refresh").clone();
        assert_eq!(refresh.status, Status::Problem);
        assert_eq!(refresh.title, "Display 2 runs at 60 Hz but supports 144 Hz");
    }

    #[test]
    fn active_virtual_display_is_a_low_problem() {
        let mut s = owner_pc_fixture();
        s.displays.push(display(
            "\\\\.\\DISPLAY9",
            60,
            60,
            false,
            true,
            "Parsec Virtual Display Adapter",
        ));
        let v = get(&run_health_check(&s, today()), "virtual-display").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::Low));
    }

    #[test]
    fn old_driver_links_to_the_vendor_and_virtual_gpus_are_ignored() {
        let mut s = owner_pc_fixture();
        s.gpus = vec![
            gpu(
                "Parsec Virtual Display Adapter",
                GpuVendor::Virtual,
                false,
                Some("2019-01-01"),
            ),
            gpu(
                "NVIDIA GeForce RTX 3060 Ti",
                GpuVendor::Nvidia,
                false,
                Some("2026-09-15"),
            ),
        ];
        assert_eq!(
            get(&run_health_check(&s, today()), "gpu-driver-age").status,
            Status::Pass
        );
        s.gpus[1].driver_date = Some("2026-01-01".into());
        let old = get(&run_health_check(&s, today()), "gpu-driver-age").clone();
        assert_eq!(old.status, Status::Problem);
        assert!(matches!(old.fix, Fix::Link { ref url, .. } if url.contains("nvidia.com")));
    }

    #[test]
    fn monitor_plugged_into_motherboard_on_a_desktop() {
        let mut s = owner_pc_fixture();
        s.gpus = vec![
            gpu(
                "Intel(R) UHD Graphics 750",
                GpuVendor::Intel,
                true,
                Some("2026-09-01"),
            ),
            gpu(
                "NVIDIA GeForce RTX 3060 Ti",
                GpuVendor::Nvidia,
                false,
                Some("2026-09-15"),
            ),
        ];
        s.displays = vec![display(
            "\\\\.\\DISPLAY1",
            60,
            60,
            true,
            false,
            "Intel(R) UHD Graphics 750",
        )];
        let v = get(&run_health_check(&s, today()), "valorant-gpu").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn hybrid_laptop_needs_high_performance_preference() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Laptop;
        s.machine.on_battery = Some(false);
        s.gpus = vec![
            gpu(
                "Intel(R) UHD Graphics",
                GpuVendor::Intel,
                true,
                Some("2026-09-01"),
            ),
            gpu(
                "NVIDIA GeForce RTX 4060 Laptop GPU",
                GpuVendor::Nvidia,
                false,
                Some("2026-09-15"),
            ),
        ];
        s.displays = vec![display(
            "\\\\.\\DISPLAY1",
            165,
            165,
            true,
            false,
            "Intel(R) UHD Graphics",
        )];
        assert_eq!(
            get(&run_health_check(&s, today()), "valorant-gpu").status,
            Status::Problem
        );
        s.windows.valorant_gpu_preference = Some(2);
        assert_eq!(
            get(&run_health_check(&s, today()), "valorant-gpu").status,
            Status::Pass
        );
    }

    #[test]
    fn laptop_on_battery_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Laptop;
        s.machine.on_battery = Some(true);
        assert_eq!(
            get(&run_health_check(&s, today()), "on-battery").status,
            Status::Problem
        );
    }

    #[test]
    fn fix_serializes_with_a_kind_tag() {
        let json = serde_json::to_string(&Fix::BiosGuide {
            topic: BiosTopic::Xmp,
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"bios-guide","topic":"xmp"}"#);
    }

    #[test]
    fn unreadable_refresh_rate_is_unknown_not_a_problem() {
        for current in [0, 1] {
            let mut s = owner_pc_fixture();
            s.displays = vec![display(
                "\\\\.\\DISPLAY1",
                current,
                144,
                true,
                false,
                "NVIDIA GeForce RTX 3060 Ti",
            )];
            let refresh = get(&run_health_check(&s, today()), "display-refresh").clone();
            assert_eq!(refresh.status, Status::Unknown, "current_hz {current}");
        }
    }

    #[test]
    fn virtual_display_does_not_count_for_refresh() {
        let mut s = owner_pc_fixture();
        s.displays = vec![
            display(
                "\\\\.\\DISPLAY1",
                360,
                360,
                true,
                false,
                "NVIDIA GeForce RTX 3060 Ti",
            ),
            display(
                "\\\\.\\DISPLAY3",
                30,
                240,
                false,
                true,
                "Parsec Virtual Display Adapter",
            ),
        ];
        assert_eq!(
            get(&run_health_check(&s, today()), "display-refresh").status,
            Status::Pass
        );
    }

    #[test]
    fn unknown_form_factor_on_a_hybrid_pc_is_unknown_not_a_problem() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Unknown;
        s.gpus = vec![
            gpu(
                "Intel(R) UHD Graphics 750",
                GpuVendor::Intel,
                true,
                Some("2026-09-01"),
            ),
            gpu(
                "NVIDIA GeForce RTX 3060 Ti",
                GpuVendor::Nvidia,
                false,
                Some("2026-09-15"),
            ),
        ];
        s.displays = vec![display(
            "\\\\.\\DISPLAY1",
            60,
            60,
            true,
            false,
            "Intel(R) UHD Graphics 750",
        )];
        let v = get(&run_health_check(&s, today()), "valorant-gpu").clone();
        assert_eq!(v.status, Status::Unknown);
        assert_eq!(v.title, "Couldn't tell which graphics chip VALORANT uses");
    }

    #[test]
    fn unknown_form_factor_without_hybrid_graphics_passes() {
        let mut s = owner_pc_fixture();
        s.machine.form_factor = FormFactor::Unknown;
        assert_eq!(
            get(&run_health_check(&s, today()), "valorant-gpu").status,
            Status::Pass
        );
    }
}
