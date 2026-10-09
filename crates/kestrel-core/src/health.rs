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
#[derive(Clone, Copy)]
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
    fn info(&self, title: impl Into<String>, detail: impl Into<String>) -> Finding {
        self.result(Status::Info, title, detail, Fix::Nothing)
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
        power_plan(s),
        game_mode(s),
        background_recording(s),
        pending_restart(s),
        windows_build(s),
        valorant_drive(s),
        drive_space(s),
        background_cpu(s),
        startup_apps(s),
        tpm(s),
        secure_boot(s),
        memory_integrity(s),
        iommu(s),
        vanguard(s),
        on_demand(s),
        wired(s),
        link_speed(s),
        adapter_power(s),
        delivery_optimization(s),
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

const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE_PERFORMANCE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

fn power_plan(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "power-plan",
        category: Category::Windows,
        severity: Severity::Medium,
    };
    let Some(plan) = &s.windows.power_plan else {
        return c.unknown("Couldn't read your power plan", "");
    };
    let guid = plan.guid.to_ascii_lowercase();
    if guid == HIGH_PERFORMANCE || guid == ULTIMATE_PERFORMANCE || plan.name.contains("Kestrel") {
        c.pass(format!("Power plan: {}", plan.name), "")
    } else {
        c.problem(
            format!("Power plan is {}", plan.name),
            "This plan lets the CPU slow down between frames, which hurts 1% lows. Kestrel's own plan arrives in stage 3; until then you can pick High performance in Control Panel → Power Options.",
            Fix::LaterStage { stage: 3 },
        )
    }
}

fn game_mode(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "game-mode",
        category: Category::Windows,
        severity: Severity::Medium,
    };
    match s.windows.game_mode {
        Some(true) => c.pass("Game Mode is on", ""),
        Some(false) => c.problem("Game Mode is off", "Game Mode tells Windows to favour the game you're playing. Ryzen X3D chips also need it to pick the right cores.", Fix::LaterStage { stage: 3 }),
        None => c.unknown("Couldn't read the Game Mode setting", ""),
    }
}

fn background_recording(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "background-recording",
        category: Category::Windows,
        severity: Severity::Medium,
    };
    match s.windows.background_recording {
        Some(true) => c.problem("Background recording is on", "Xbox Game Bar keeps recording gameplay in the background (\"Record what happened\"), which costs FPS.", Fix::LaterStage { stage: 3 }),
        Some(false) => c.pass("Background recording is off", ""),
        None => c.unknown("Couldn't read the background recording setting", ""),
    }
}

fn pending_restart(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "pending-restart",
        category: Category::Windows,
        severity: Severity::Low,
    };
    match s.windows.pending_restart {
        Some(true) => c.problem("Windows is waiting to restart", "Updates stay half-installed until you restart, and can keep working in the background while you play.", steps(&["Save your work and restart the PC."])),
        Some(false) => c.pass("No restart pending", ""),
        None => c.unknown("Couldn't tell whether a restart is pending", ""),
    }
}

fn windows_build(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "windows-build",
        category: Category::Windows,
        severity: Severity::Medium,
    };
    let build = s.os.build;
    let update = || {
        steps(&[
            "Open Settings → Windows Update.",
            "Install the latest version Windows offers.",
        ])
    };
    match build {
        0 => c.unknown("Couldn't read your Windows version", ""),
        b if b >= 26100 => c.pass(format!("Windows 11 {}", s.os.display_version.clone().unwrap_or_default()).trim_end().to_string(), format!("Build {b}.")),
        b if b >= 22000 => c.problem(format!("Windows 11 is out of date (build {b})"), "Riot can require Windows 11 24H2 (build 26100) or newer for some accounts.", update()),
        b if b >= 19045 => Check { severity: Severity::Low, ..c }.problem(
            "You're on Windows 10",
            "VALORANT still runs on Windows 10, but Riot can require Windows 11 for some accounts, and Windows 10's extended security updates for home PCs are reported to end on 13 October 2026. Kestrel runs here but isn't tested on Windows 10.",
            Fix::Link { label: "About upgrading to Windows 11".into(), url: "https://www.microsoft.com/windows/get-windows-11".into() },
        ),
        b if b >= 19041 => c.problem(format!("Windows 10 needs updating (build {b})"), "Riot can require Windows 10 22H2 (build 19045) or newer for some accounts.", update()),
        b => Check { severity: Severity::High, ..c }.problem(format!("Windows is too old for VALORANT (build {b})"), "VALORANT needs Windows 10 build 19041 or newer.", update()),
    }
}

fn valorant_drive(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "valorant-drive",
        category: Category::Storage,
        severity: Severity::High,
    };
    let Some(v) = &s.storage.valorant else {
        return c.unknown("Couldn't find VALORANT", "Kestrel looks in Riot's install records. If VALORANT isn't installed, the drive checks are skipped.");
    };
    match v.media {
        DriveMedia::Hdd => c.problem(
            format!("VALORANT is on a hard drive ({}:)", v.drive_letter),
            "Hard drives load maps and textures slowly, which causes stutter. Moving VALORANT to an SSD fixes it.",
            steps(&["In the Riot Client, uninstall VALORANT.", "Install it again and choose a folder on your SSD."]),
        ),
        DriveMedia::Nvme => c.pass(format!("VALORANT is on an NVMe SSD ({}:)", v.drive_letter), ""),
        DriveMedia::Ssd => c.pass(format!("VALORANT is on an SSD ({}:)", v.drive_letter), ""),
        DriveMedia::Unknown => c.unknown(format!("Couldn't tell what kind of drive {}: is", v.drive_letter), ""),
    }
}

fn drive_space(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "drive-space",
        category: Category::Storage,
        severity: Severity::Low,
    };
    let Some(v) = &s.storage.valorant else {
        return c.unknown("Couldn't check free space", "VALORANT wasn't found.");
    };
    if v.total_gb <= 0.0 {
        return c.unknown(
            format!("Couldn't read free space on {}:", v.drive_letter),
            "",
        );
    }
    if v.free_gb < 20.0 || v.free_gb / v.total_gb < 0.10 {
        c.problem(
            format!("Drive {}: is nearly full ({:.0} GB free)", v.drive_letter, v.free_gb),
            "Windows and VALORANT need free space for updates and the shader cache. Keep at least 20 GB free.",
            steps(&["Open Settings → System → Storage → Cleanup recommendations."]),
        )
    } else {
        c.pass(
            format!("Drive {}: has {:.0} GB free", v.drive_letter, v.free_gb),
            "",
        )
    }
}

fn background_cpu(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "background-cpu",
        category: Category::Background,
        severity: Severity::Medium,
    };
    let Some(list) = &s.background.busy_processes else {
        return c.unknown("Couldn't measure background activity", "");
    };
    let busy: Vec<&BusyProcess> = list.iter().filter(|p| p.cpu_percent >= 5.0).collect();
    if busy.is_empty() {
        return c.pass("Nothing heavy is running in the background", "");
    }
    let names = busy
        .iter()
        .map(|p| format!("{} ({:.0}%)", p.name, p.cpu_percent))
        .collect::<Vec<_>>()
        .join(", ");
    let who = if busy.len() == 1 {
        "app is"
    } else {
        "apps are"
    };
    c.problem(
        format!("{} {who} using CPU in the background", busy.len()),
        format!("Busiest: {names}. Close them before you play."),
        steps(&["Close these apps before you play, or stop them starting with Windows (Task Manager → Startup apps)."]),
    )
}

fn startup_apps(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "startup-apps",
        category: Category::Background,
        severity: Severity::Low,
    };
    match s.windows.startup_app_count {
        Some(n) if n > 10 => c.problem(
            format!("{n} apps start with Windows"),
            "Each one uses memory and CPU in the background. Kestrel's startup manager arrives in stage 4; until then, turn off what you don't need in Task Manager → Startup apps.",
            Fix::LaterStage { stage: 4 },
        ),
        Some(n) => c.pass(format!("{n} apps start with Windows"), ""),
        None => c.unknown("Couldn't count startup apps", ""),
    }
}

/// On Windows 11 Vanguard requires TPM and Secure Boot; on Windows 10 only flagged accounts do.
fn vanguard_requirement(s: &SystemSnapshot, id: &'static str) -> Check {
    let severity = if s.os.build >= 22000 {
        Severity::High
    } else {
        Severity::Low
    };
    Check {
        id,
        category: Category::Vanguard,
        severity,
    }
}

fn tpm(s: &SystemSnapshot) -> Finding {
    let c = vanguard_requirement(s, "tpm");
    match s.security.tpm2 {
        Some(true) => c.pass("TPM 2.0 is on", ""),
        Some(false) => c.problem(
            "TPM 2.0 is off or missing",
            "VALORANT on Windows 11 needs TPM 2.0. In the BIOS it's often called fTPM (AMD) or PTT (Intel).",
            steps(&["Restart into the BIOS.", "Turn on fTPM (AMD) or Intel PTT (Intel), usually under Advanced or Security.", "Save and restart."]),
        ),
        None => c.unknown("Couldn't check TPM 2.0", "This check needs Kestrel to run as administrator."),
    }
}

fn secure_boot(s: &SystemSnapshot) -> Finding {
    let c = vanguard_requirement(s, "secure-boot");
    match s.security.secure_boot {
        Some(true) => c.pass("Secure Boot is on", ""),
        Some(false) => c.problem(
            "Secure Boot is off",
            "VALORANT on Windows 11 needs Secure Boot.",
            steps(&[
                "Restart into the BIOS.",
                "In the Boot section, set Secure Boot to Enabled (Windows UEFI mode).",
                "Save and restart.",
            ]),
        ),
        None => c.unknown("Couldn't check Secure Boot", ""),
    }
}

fn memory_integrity(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "memory-integrity",
        category: Category::Vanguard,
        severity: Severity::Info,
    };
    match s.security.hvci_running {
        Some(true) => c.info("Memory Integrity is on", "Good for security, and needed for Vanguard's On-Demand mode. It can cost a few percent of FPS; Kestrel's Extreme tier will offer to turn it off, with a warning."),
        Some(false) => c.info("Memory Integrity is off", "VALORANT works fine like this. Riot can require it on for flagged accounts, and Vanguard's On-Demand mode needs it."),
        None => c.unknown("Couldn't check Memory Integrity", ""),
    }
}

fn iommu(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "iommu",
        category: Category::Vanguard,
        severity: Severity::Info,
    };
    match s.security.iommu {
        Some(true) => c.info("IOMMU is on", "Needed for Vanguard's On-Demand mode."),
        Some(false) => c.result(
            Status::Info,
            "IOMMU is off",
            "Only needed for Vanguard's On-Demand mode, or if Riot asks for it. It's called VT-d on Intel and IOMMU or AMD-Vi on AMD.",
            Fix::BiosGuide { topic: BiosTopic::Iommu },
        ),
        None => c.unknown("Couldn't check IOMMU", ""),
    }
}

fn vanguard(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "vanguard",
        category: Category::Vanguard,
        severity: Severity::High,
    };
    match s.security.vanguard {
        ServiceState::Running => c.pass("Vanguard is installed and running", ""),
        ServiceState::Stopped => c.pass("Vanguard is installed", "It starts when needed."),
        ServiceState::NotInstalled => c.problem(
            "Vanguard isn't installed",
            "VALORANT won't start without Riot Vanguard.",
            steps(&[
                "Open the Riot Client.",
                "Launch VALORANT; the Riot Client reinstalls Vanguard and asks you to restart.",
            ]),
        ),
        ServiceState::Unknown => c.unknown("Couldn't check Vanguard", ""),
    }
}

fn on_demand(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "on-demand",
        category: Category::Vanguard,
        severity: Severity::Info,
    };
    if s.os.build == 0 {
        return c.unknown("Couldn't check Vanguard On-Demand eligibility", "");
    }
    let sec = &s.security;
    let needs = [
        ("Windows 11 25H2", Some(s.os.build >= 26200)),
        ("Secure Boot", sec.secure_boot),
        ("TPM 2.0", sec.tpm2),
        ("VBS", sec.vbs_running),
        ("Memory Integrity", sec.hvci_running),
        ("IOMMU", sec.iommu),
    ];
    let missing: Vec<&str> = needs
        .iter()
        .filter(|(_, v)| *v == Some(false))
        .map(|(name, _)| *name)
        .collect();
    if !missing.is_empty() {
        c.info(
            "Not eligible for Vanguard On-Demand mode",
            format!(
                "Missing: {}. On-Demand is optional; VALORANT works without it.",
                missing.join(", ")
            ),
        )
    } else if needs.iter().any(|(_, v)| v.is_none()) {
        c.unknown("Couldn't check Vanguard On-Demand eligibility", "")
    } else {
        c.info(
            "This PC qualifies for Vanguard On-Demand mode",
            "On-Demand runs Vanguard only while you play.",
        )
    }
}

fn connected(s: &SystemSnapshot, kind: NetKind) -> Option<&NetAdapter> {
    s.network
        .adapters
        .iter()
        .find(|a| a.connected && a.kind == kind)
}

fn wired(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "wired",
        category: Category::Network,
        severity: Severity::Medium,
    };
    if connected(s, NetKind::Ethernet).is_some() {
        c.pass("Connected by cable", "")
    } else if connected(s, NetKind::Wifi).is_some() {
        c.problem(
            "Playing over Wi-Fi",
            "Wi-Fi adds latency spikes and packet loss. Use an Ethernet cable if you can.",
            steps(&["Connect the PC to the router with an Ethernet cable."]),
        )
    } else {
        c.unknown("Couldn't find an active network connection", "")
    }
}

fn link_speed(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "link-speed",
        category: Category::Network,
        severity: Severity::Low,
    };
    match connected(s, NetKind::Ethernet).map(|a| a.link_mbps) {
        Some(Some(mbps)) if mbps < 1000 => c.problem(
            format!("Network link is only {mbps} Mbps"),
            "Gigabit Ethernet should connect at 1000 Mbps or more. A slower link usually means a damaged cable or a bad port.",
            steps(&["Try another Ethernet cable (Cat 5e or better) or another port on the router."]),
        ),
        Some(Some(mbps)) => c.pass(format!("Network link: {mbps} Mbps"), ""),
        Some(None) => c.unknown("Couldn't read the network link speed", ""),
        None => c.info("No cable connected, so link speed isn't checked", ""),
    }
}

fn adapter_power(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "adapter-power",
        category: Category::Network,
        severity: Severity::Low,
    };
    let active: Vec<&NetAdapter> = s.network.adapters.iter().filter(|a| a.connected).collect();
    if active.iter().any(|a| a.power_saving == Some(true)) {
        c.problem("Network adapter power saving is on", "Windows can let the adapter doze, which causes small latency spikes. Kestrel's Competitive tier turns this off in stage 4.", Fix::LaterStage { stage: 4 })
    } else if !active.is_empty() && active.iter().all(|a| a.power_saving == Some(false)) {
        c.pass("Network adapter power saving is off", "")
    } else {
        c.unknown("Couldn't check network power saving", "")
    }
}

fn delivery_optimization(s: &SystemSnapshot) -> Finding {
    let c = Check {
        id: "delivery-optimization",
        category: Category::Network,
        severity: Severity::Low,
    };
    match s.network.delivery_optimization_internet {
        Some(true) => c.problem(
            "Windows Update uploads to other PCs over the internet",
            "Your PC shares updates with strangers, which uses upload bandwidth while you play.",
            Fix::LaterStage { stage: 4 },
        ),
        Some(false) => c.pass("Windows Update doesn't upload to the internet", ""),
        None => c.unknown("Couldn't read the Windows Update sharing setting", ""),
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

    #[test]
    fn owner_pc_full_results() {
        let f = run_health_check(&owner_pc_fixture(), today());
        assert_eq!(f.len(), 27);
        let with = |status: Status| -> Vec<&str> {
            f.iter()
                .filter(|x| x.status == status)
                .map(|x| x.id.as_str())
                .collect()
        };
        assert_eq!(
            with(Status::Problem),
            vec!["bios-age", "power-plan", "adapter-power"]
        );
        assert_eq!(
            with(Status::Info),
            vec!["memory-integrity", "iommu", "on-demand"]
        );
        assert!(with(Status::Unknown).is_empty());
    }

    #[test]
    fn every_check_has_a_unique_id() {
        let f = run_health_check(&owner_pc_fixture(), today());
        let mut ids: Vec<&str> = f.iter().map(|x| x.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), f.len());
    }

    #[test]
    fn windows_versions() {
        let mut s = owner_pc_fixture();
        s.os.build = 19045;
        let w10 = get(&run_health_check(&s, today()), "windows-build").clone();
        assert_eq!((w10.status, w10.severity), (Status::Problem, Severity::Low));
        assert!(matches!(w10.fix, Fix::Link { .. }));
        s.os.build = 22631;
        let old11 = get(&run_health_check(&s, today()), "windows-build").clone();
        assert_eq!(
            (old11.status, old11.severity),
            (Status::Problem, Severity::Medium)
        );
        s.os.build = 18363;
        assert_eq!(
            get(&run_health_check(&s, today()), "windows-build").severity,
            Severity::High
        );
    }

    #[test]
    fn valorant_on_a_hard_drive_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        s.storage.valorant.as_mut().unwrap().media = DriveMedia::Hdd;
        let d = get(&run_health_check(&s, today()), "valorant-drive").clone();
        assert_eq!((d.status, d.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn valorant_missing_makes_storage_checks_unknown() {
        let mut s = owner_pc_fixture();
        s.storage.valorant = None;
        let f = run_health_check(&s, today());
        assert_eq!(get(&f, "valorant-drive").status, Status::Unknown);
        assert_eq!(get(&f, "drive-space").status, Status::Unknown);
    }

    #[test]
    fn nearly_full_drive() {
        let mut s = owner_pc_fixture();
        s.storage.valorant.as_mut().unwrap().free_gb = 12.0;
        assert_eq!(
            get(&run_health_check(&s, today()), "drive-space").status,
            Status::Problem
        );
    }

    #[test]
    fn busy_background_apps_are_named() {
        let mut s = owner_pc_fixture();
        s.background.busy_processes = Some(vec![
            BusyProcess {
                name: "chrome.exe".into(),
                cpu_percent: 12.0,
            },
            BusyProcess {
                name: "discord.exe".into(),
                cpu_percent: 2.0,
            },
        ]);
        let b = get(&run_health_check(&s, today()), "background-cpu").clone();
        assert_eq!(b.status, Status::Problem);
        assert_eq!(b.title, "1 app is using CPU in the background");
        assert!(b.detail.contains("chrome.exe (12%)") && !b.detail.contains("discord"));
    }

    #[test]
    fn wifi_is_a_problem_and_link_speed_is_skipped() {
        let mut s = owner_pc_fixture();
        s.network.adapters = vec![NetAdapter {
            name: "Wi-Fi".into(),
            kind: NetKind::Wifi,
            link_mbps: Some(866),
            connected: true,
            power_saving: Some(false),
        }];
        let f = run_health_check(&s, today());
        assert_eq!(get(&f, "wired").status, Status::Problem);
        assert_eq!(get(&f, "link-speed").status, Status::Info);
    }

    #[test]
    fn slow_ethernet_link_is_a_problem() {
        let mut s = owner_pc_fixture();
        s.network.adapters[0].link_mbps = Some(100);
        assert_eq!(
            get(&run_health_check(&s, today()), "link-speed").status,
            Status::Problem
        );
    }

    #[test]
    fn on_demand_eligible_when_everything_is_on() {
        let mut s = owner_pc_fixture();
        s.security.vbs_running = Some(true);
        s.security.hvci_running = Some(true);
        s.security.iommu = Some(true);
        let o = get(&run_health_check(&s, today()), "on-demand").clone();
        assert_eq!(o.status, Status::Info);
        assert!(o.title.contains("qualifies"), "{}", o.title);
    }

    #[test]
    fn missing_vanguard_is_a_high_problem() {
        let mut s = owner_pc_fixture();
        s.security.vanguard = ServiceState::NotInstalled;
        let v = get(&run_health_check(&s, today()), "vanguard").clone();
        assert_eq!((v.status, v.severity), (Status::Problem, Severity::High));
    }

    #[test]
    fn tpm_off_is_high_on_windows_11_and_low_on_windows_10() {
        let mut s = owner_pc_fixture();
        s.security.tpm2 = Some(false);
        assert_eq!(
            get(&run_health_check(&s, today()), "tpm").severity,
            Severity::High
        );
        s.os.build = 19045;
        assert_eq!(
            get(&run_health_check(&s, today()), "tpm").severity,
            Severity::Low
        );
    }
}
