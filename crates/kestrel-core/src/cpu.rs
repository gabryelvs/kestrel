//! CPUID decoding: the CPU's maker, family, model and stepping, the instruction sets Windows lets programs use,
//! and the codename table. The probe runs the instruction; everything here works on the register values.
use crate::snapshot::CpuVendor;
use std::ops::RangeInclusive;

/// The registers the probe reads with `CPUID` and `XGETBV`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuidRaw {
    /// Leaf 0 vendor string, e.g. "GenuineIntel", "AuthenticAMD".
    pub vendor: String,
    /// Leaf 0 EAX: the highest standard leaf.
    pub max_leaf: u32,
    /// Leaf 1 EAX, EBX, ECX, EDX.
    pub leaf1: [u32; 4],
    /// Leaf 7 sub-leaf 0 EAX, EBX, ECX, EDX; zeros when `max_leaf` < 7.
    pub leaf7: [u32; 4],
    /// Leaf 7 sub-leaf 1 EAX; 0 when `max_leaf` < 7 or `leaf7[0]` < 1.
    pub leaf7_1_eax: u32,
    /// Leaf 0x16 EAX, EBX, ECX when `max_leaf` >= 0x16.
    pub leaf16: Option<[u32; 3]>,
    /// Leaves 0x80000002–4, NULs removed, trimmed; empty when unavailable.
    pub brand: String,
    /// XGETBV(0) when leaf 1 ECX bit 27 (OSXSAVE) is set.
    pub xcr0: Option<u64>,
}

/// What the registers say about the CPU.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuIdentity {
    pub vendor: CpuVendor,
    /// Display family and model: the extended fields already added.
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    /// The sets the CPU has and Windows enables, in this order: "SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI",
    /// "AMX".
    pub instructions: Vec<String>,
    /// Leaf 1 ECX bit 31: Windows runs on a hypervisor (Hyper-V, VBS) or inside a virtual machine.
    pub hypervisor: bool,
    /// Intel leaf 0x16 EAX[15:0] when non-zero.
    pub base_mhz: Option<u32>,
    /// Intel leaf 0x16 EBX[15:0] when non-zero.
    pub max_mhz: Option<u32>,
}

// Feature bits, as in Intel's SDM (Vol. 2A, CPUID) and Linux's arch/x86/include/asm/cpufeatures.h, where word 4 is
// leaf 1 ECX, word 9 leaf 7.0 EBX, word 12 leaf 7.1 EAX and word 18 leaf 7.0 EDX.
const SSE4_2: u32 = 1 << 20; // leaf 1 ECX: X86_FEATURE_XMM4_2 (4*32+20)
const OSXSAVE: u32 = 1 << 27; // leaf 1 ECX: X86_FEATURE_OSXSAVE (4*32+27)
const AVX: u32 = 1 << 28; // leaf 1 ECX: X86_FEATURE_AVX (4*32+28)
const HYPERVISOR: u32 = 1 << 31; // leaf 1 ECX: X86_FEATURE_HYPERVISOR (4*32+31)
const AVX2: u32 = 1 << 5; // leaf 7.0 EBX: X86_FEATURE_AVX2 (9*32+5)
const AVX512F: u32 = 1 << 16; // leaf 7.0 EBX: X86_FEATURE_AVX512F (9*32+16)
const AMX_TILE: u32 = 1 << 24; // leaf 7.0 EDX: X86_FEATURE_AMX_TILE (18*32+24)
const AVX_VNNI: u32 = 1 << 4; // leaf 7.1 EAX: X86_FEATURE_AVX_VNNI (12*32+4)

// XCR0: the register state the OS saves on a task switch (Linux's arch/x86/include/asm/fpu/types.h, enum
// xfeature). A set counts only when its state is saved, as the SDM's AVX and AVX-512 detection steps require.
const STATE_AVX: u64 = (1 << 1) | (1 << 2); // XFEATURE_SSE, XFEATURE_YMM
const STATE_AVX512: u64 = (1 << 5) | (1 << 6) | (1 << 7); // XFEATURE_OPMASK, _ZMM_Hi256, _Hi16_ZMM
const STATE_AMX: u64 = (1 << 17) | (1 << 18); // XFEATURE_XTILE_CFG, XFEATURE_XTILE_DATA

/// Decodes the registers. As Intel's and AMD's manuals define the display values, the extended family is added
/// when the base family is 0xF, and the extended model becomes the model's high nibble when the base family is 6
/// or 0xF.
pub fn decode(raw: &CpuidRaw) -> CpuIdentity {
    let vendor = match raw.vendor.as_str() {
        "GenuineIntel" => CpuVendor::Intel,
        "AuthenticAMD" => CpuVendor::Amd,
        _ => CpuVendor::Other,
    };
    let [eax, _, ecx, _] = raw.leaf1;
    let base_family = (eax >> 8) & 0xF;
    let base_model = (eax >> 4) & 0xF;
    let family = match base_family {
        0xF => base_family + ((eax >> 20) & 0xFF),
        _ => base_family,
    };
    let model = match base_family {
        6 | 0xF => (((eax >> 16) & 0xF) << 4) | base_model,
        _ => base_model,
    };

    let has = |register: u32, bit: u32| (register & bit) != 0;
    let os_saves =
        |state: u64| has(ecx, OSXSAVE) && raw.xcr0.is_some_and(|xcr0| (xcr0 & state) == state);
    let [_, ebx7, _, edx7] = raw.leaf7;
    let avx = has(ecx, AVX) && os_saves(STATE_AVX);
    let avx512 = avx && has(ebx7, AVX512F) && os_saves(STATE_AVX512);
    let sets = [
        ("SSE4.2", has(ecx, SSE4_2)),
        ("AVX", avx),
        ("AVX2", avx && has(ebx7, AVX2)),
        ("AVX-512", avx512),
        ("AVX-VNNI", avx && has(raw.leaf7_1_eax, AVX_VNNI)),
        ("AMX", has(edx7, AMX_TILE) && os_saves(STATE_AMX)),
    ];

    // Leaf 0x16 is Intel's; AMD keeps it reserved.
    let mhz = |value: u32| Some(value & 0xFFFF).filter(|&mhz| mhz > 0);
    let (base_mhz, max_mhz) = match (vendor, raw.leaf16) {
        (CpuVendor::Intel, Some([base, max, _])) => (mhz(base), mhz(max)),
        _ => (None, None),
    };

    CpuIdentity {
        vendor,
        family,
        model,
        stepping: eax & 0xF,
        instructions: sets
            .into_iter()
            .filter(|&(_, on)| on)
            .map(|(name, _)| name.to_string())
            .collect(),
        hypervisor: has(ecx, HYPERVISOR),
        base_mhz,
        max_mhz,
    }
}

/// A CPU's codename, and the socket of its desktop parts (None for chips sold only soldered in laptops and
/// handhelds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codename {
    pub name: &'static str,
    pub socket: Option<&'static str>,
}

/// The codename of a display family, model and stepping from `decode`, or None when the table doesn't list it.
pub fn codename(vendor: CpuVendor, family: u32, model: u32, stepping: u32) -> Option<Codename> {
    let rows = match vendor {
        CpuVendor::Intel => INTEL,
        CpuVendor::Amd => AMD,
        CpuVendor::Other => return None,
    };
    rows.iter()
        .find(|row| row.family == family && row.model == model && row.steppings.contains(&stepping))
        .map(|row| Codename {
            name: row.name,
            socket: row.socket,
        })
}

/// One table row: a family and model, the steppings it covers, and what that CPU is.
struct Row {
    family: u32,
    model: u32,
    steppings: RangeInclusive<u32>,
    name: &'static str,
    socket: Option<&'static str>,
}

const fn row(
    family: u32,
    model: u32,
    steppings: RangeInclusive<u32>,
    name: &'static str,
    socket: Option<&'static str>,
) -> Row {
    Row {
        family,
        model,
        steppings,
        name,
        socket,
    }
}

/// Every stepping: the model alone says which CPU it is.
const ANY: RangeInclusive<u32> = 0..=0xF;

const LGA1151: Option<&str> = Some("LGA1151");
const LGA1200: Option<&str> = Some("LGA1200");
const LGA1700: Option<&str> = Some("LGA1700");
const LGA1851: Option<&str> = Some("LGA1851");
const AM4: Option<&str> = Some("AM4");
const AM5: Option<&str> = Some("AM5");
const STRX4: Option<&str> = Some("sTRX4");
const SWRX8: Option<&str> = Some("sWRX8");
const STR5: Option<&str> = Some("sTR5");
/// Sold only soldered: laptops, handhelds, mini PCs.
const MOBILE: Option<&str> = None;

/// Intel Core client CPUs, 6th gen (Skylake) to Core Ultra series 3 (Panther Lake) and Core series 3 (Wildcat
/// Lake). Sources:
/// - Linux, arch/x86/include/asm/intel-family.h: which microarchitecture each model is, and the stepping split of
///   models 0x8E and 0x9E.
/// - Intel's microcode release notes (github.com/intel/Intel-Linux-Processor-Microcode-Data-Files, releasenote.md):
///   the products and segments behind each family-model-stepping, e.g. "RKL-S B0 06-a7-01 Core Gen11".
/// - InstLatx64's CPUID dumps (github.com/InstLatx64/InstLatx64): a real chip's signature for every row, and the
///   newest names (Bartlett Lake, Twin Lake, Wildcat Lake).
/// - libcpuid's src/recog_intel.c (github.com/anrieff/libcpuid): the segment names, e.g. Tiger Lake-UP3/UP4.
///
/// A model that desktop and laptop chips share names both segments (S/H, S/HX) and gives the desktop socket.
/// Kaby Lake-X (Core i7-7740X, LGA2066) shares 06-9E-09 with Kaby Lake-S and shows LGA1151.
const INTEL: &[Row] = &[
    // Skylake, Kaby Lake, Coffee Lake, Whiskey Lake, Amber Lake, Comet Lake (6th to 10th gen).
    row(6, 0x4E, ANY, "Skylake-U/Y", MOBILE),
    row(6, 0x5E, ANY, "Skylake-S/H", LGA1151),
    row(6, 0x8E, 9..=9, "Kaby Lake-U/Y / Amber Lake-Y", MOBILE),
    row(6, 0x8E, 10..=10, "Kaby Lake-R / Coffee Lake-U", MOBILE),
    row(6, 0x8E, 11..=11, "Whiskey Lake-U", MOBILE),
    row(
        6,
        0x8E,
        12..=12,
        "Whiskey Lake-U / Amber Lake-Y / Comet Lake-U",
        MOBILE,
    ),
    row(6, 0x9E, 9..=9, "Kaby Lake-S/H", LGA1151),
    row(6, 0x9E, 10..=13, "Coffee Lake-S/H", LGA1151),
    row(6, 0xA5, 2..=2, "Comet Lake-H", MOBILE),
    row(6, 0xA5, 3..=5, "Comet Lake-S", LGA1200),
    row(6, 0xA6, ANY, "Comet Lake-U", MOBILE),
    // Cannon Lake, Ice Lake, Lakefield, Tiger Lake, Rocket Lake (8th to 11th gen on new cores).
    row(6, 0x66, ANY, "Cannon Lake-U", MOBILE),
    row(6, 0x7E, ANY, "Ice Lake-U/Y", MOBILE),
    row(6, 0x8A, ANY, "Lakefield", MOBILE),
    row(6, 0x8C, ANY, "Tiger Lake-UP3/UP4", MOBILE),
    row(6, 0x8D, ANY, "Tiger Lake-H", MOBILE),
    row(6, 0xA7, ANY, "Rocket Lake-S", LGA1200),
    // Alder Lake, Raptor Lake, Bartlett Lake (12th to 14th gen, Core Series 2 with P-cores).
    row(6, 0x97, ANY, "Alder Lake-S/HX", LGA1700),
    row(6, 0x9A, ANY, "Alder Lake-H/P/U", MOBILE),
    row(6, 0xBE, ANY, "Alder Lake-N / Twin Lake", MOBILE),
    row(6, 0xB7, 1..=1, "Raptor Lake-S/HX", LGA1700),
    row(6, 0xB7, 4..=4, "Bartlett Lake-S", LGA1700),
    row(6, 0xBA, ANY, "Raptor Lake-H/P/U", MOBILE),
    row(6, 0xBF, ANY, "Raptor Lake-S/HX", LGA1700),
    row(6, 0xD7, ANY, "Bartlett Lake-S", LGA1700),
    // Core Ultra series 1 to 3, Core series 3.
    row(6, 0xAA, ANY, "Meteor Lake-H/U", MOBILE),
    row(6, 0xC6, ANY, "Arrow Lake-S/HX", LGA1851),
    row(6, 0xC5, ANY, "Arrow Lake-H", MOBILE),
    row(6, 0xB5, ANY, "Arrow Lake-U", MOBILE),
    row(6, 0xBD, ANY, "Lunar Lake", MOBILE),
    row(6, 0xCC, ANY, "Panther Lake", MOBILE),
    row(6, 0xD5, ANY, "Wildcat Lake", MOBILE),
];

/// AMD Ryzen and Threadripper, Zen (family 0x17) to Zen 5 (family 0x1A), desktop and mobile. Sources:
/// - Linux, arch/x86/kernel/cpu/amd.c (Zen generation by model range) and drivers/hwmon/k10temp.c (model comments,
///   e.g. "0x8: Zen3 TR Chagall").
/// - libcpuid's src/recog_amd.c (github.com/anrieff/libcpuid): the codename of each model.
/// - InstLatx64's CPUID dumps (github.com/InstLatx64/InstLatx64): a real chip's signature for every row but
///   Chagall (whose model and stepping are in amd.c's microcode table), and the names libcpuid lacks (Phoenix 2,
///   Hawk Point 2, Gorgon Point).
/// - CPU-Z validations (valid.x86.fr): Ryzen 9 9955HX as Fire Range (1A/44), Ryzen 5 8500G as Phoenix 2 (19/78).
///
/// A die sold as desktop and laptop chips names both (Raphael / Dragon Range) and gives the desktop socket.
/// Threadripper 1000 and 2000 (TR4) share 17/01 and 17/08 with Ryzen, and Threadripper PRO 3000WX (sWRX8) shares
/// 17/31 with Threadripper 3000, so those show the Ryzen or Threadripper 3000 name and socket.
const AMD: &[Row] = &[
    // Zen, Zen+, Zen 2.
    row(0x17, 0x01, ANY, "Summit Ridge", AM4),
    row(0x17, 0x08, ANY, "Pinnacle Ridge", AM4),
    row(0x17, 0x11, ANY, "Raven Ridge", AM4),
    row(0x17, 0x18, ANY, "Picasso", AM4),
    row(0x17, 0x20, ANY, "Dali", MOBILE),
    row(0x17, 0x31, ANY, "Castle Peak", STRX4),
    row(0x17, 0x60, ANY, "Renoir", AM4),
    row(0x17, 0x68, ANY, "Lucienne", MOBILE),
    row(0x17, 0x71, ANY, "Matisse", AM4),
    row(0x17, 0x90, ANY, "Van Gogh", MOBILE),
    row(0x17, 0x91, ANY, "Van Gogh", MOBILE),
    row(0x17, 0xA0, ANY, "Mendocino", MOBILE),
    // Zen 3, Zen 3+, Zen 4.
    row(0x19, 0x08, ANY, "Chagall", SWRX8),
    row(0x19, 0x18, ANY, "Storm Peak", STR5),
    row(0x19, 0x21, ANY, "Vermeer", AM4),
    row(0x19, 0x44, ANY, "Rembrandt", MOBILE),
    row(0x19, 0x50, ANY, "Cezanne / Barcelo", AM4),
    row(0x19, 0x61, ANY, "Raphael / Dragon Range", AM5),
    row(0x19, 0x74, ANY, "Phoenix", MOBILE),
    row(0x19, 0x75, ANY, "Phoenix / Hawk Point", AM5),
    row(0x19, 0x78, ANY, "Phoenix 2", AM5),
    row(0x19, 0x7C, ANY, "Hawk Point 2", MOBILE),
    // Zen 5.
    row(0x1A, 0x08, ANY, "Shimada Peak", STR5),
    row(0x1A, 0x24, ANY, "Strix Point / Gorgon Point", MOBILE),
    row(0x1A, 0x44, ANY, "Granite Ridge / Fire Range", AM5),
    row(0x1A, 0x60, ANY, "Krackan Point / Gorgon Point", MOBILE),
    row(0x1A, 0x70, ANY, "Strix Halo", MOBILE),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::CpuVendor::{Amd, Intel, Other};

    const INTEL_ID: &str = "GenuineIntel";
    const AMD_ID: &str = "AuthenticAMD";

    // Feature bits, written out here independently of the constants under test.
    const ECX_SSE4_2: u32 = 1 << 20;
    const ECX_OSXSAVE: u32 = 1 << 27;
    const ECX_AVX: u32 = 1 << 28;
    const ECX_HYPERVISOR: u32 = 1 << 31;
    const EBX7_AVX2: u32 = 1 << 5;
    const EBX7_AVX512F: u32 = 1 << 16;
    const EDX7_AMX_TILE: u32 = 1 << 24;
    const EAX7_1_AVX_VNNI: u32 = 1 << 4;
    /// XCR0 with x87 (bit 0), SSE (1) and AVX (2) state.
    const STATE_AVX: u64 = 0b111;
    /// Plus the AVX-512 state: opmask (5), upper halves of ZMM0–15 (6), ZMM16–31 (7).
    const STATE_AVX512: u64 = 0b1110_0111;
    /// The AMX state: tile configuration (17) and tile data (18).
    const STATE_AMX: u64 = 0b11 << 17;

    /// The owner's CPU: leaves 0, 1, 7 and 0x16 of the i9-11900K in InstLatx64's dump
    /// GenuineIntel00A0671_RocketLake_01, with the XCR0 Windows sets on an AVX-512 CPU.
    fn i9_11900k() -> CpuidRaw {
        CpuidRaw {
            vendor: INTEL_ID.into(),
            max_leaf: 0x1B,
            leaf1: [0x000A0671, 0x00100800, 0x7FFAFBFF, 0xBFEBFBFF],
            leaf7: [0x00000002, 0xF2BF67EB, 0x00405F4E, 0xBC000410],
            leaf7_1_eax: 0,
            leaf16: Some([0x0DAC, 0x14B4, 0x0064]),
            brand: "11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz".into(),
            xcr0: Some(0xE7),
        }
    }

    /// A Ryzen 9 9950X (Zen 5) from InstLatx64's AuthenticAMD0B40F40_K20_GraniteRidge_02: its highest standard
    /// leaf is 0x10, so there is no leaf 0x16.
    fn ryzen_9_9950x() -> CpuidRaw {
        CpuidRaw {
            vendor: AMD_ID.into(),
            max_leaf: 0x10,
            leaf1: [0x00B40F40, 0x00200800, 0x7ED8320B, 0x178BFBFF],
            leaf7: [0x00000001, 0xF1BF97AB, 0x19405FCE, 0x10000110],
            leaf7_1_eax: 0x00000030,
            leaf16: None,
            brand: "AMD Ryzen 9 9950X 16-Core Processor".into(),
            xcr0: Some(0xE7),
        }
    }

    /// A CPU with this leaf 1 EAX and no feature bits.
    fn signature(vendor: &str, eax: u32) -> CpuidRaw {
        CpuidRaw {
            vendor: vendor.into(),
            max_leaf: 0x10,
            leaf1: [eax, 0, 0, 0],
            ..CpuidRaw::default()
        }
    }

    /// The instruction sets of an Intel CPU with these feature registers: leaf 1 ECX, leaf 7.0 EBX and EDX, leaf
    /// 7.1 EAX, and XCR0.
    fn sets(ecx: u32, ebx7: u32, edx7: u32, eax7_1: u32, xcr0: Option<u64>) -> Vec<String> {
        decode(&CpuidRaw {
            leaf1: [0x000A0671, 0, ecx, 0],
            leaf7: [1, ebx7, 0, edx7],
            leaf7_1_eax: eax7_1,
            xcr0,
            ..signature(INTEL_ID, 0x000A0671)
        })
        .instructions
    }

    #[test]
    fn decodes_the_owners_i9_11900k() {
        assert_eq!(
            decode(&i9_11900k()),
            CpuIdentity {
                vendor: Intel,
                family: 6,
                model: 0xA7,
                stepping: 1,
                instructions: vec![
                    "SSE4.2".into(),
                    "AVX".into(),
                    "AVX2".into(),
                    "AVX-512".into()
                ],
                hypervisor: false,
                base_mhz: Some(3500),
                max_mhz: Some(5300),
            }
        );
    }

    #[test]
    fn the_owners_cpu_is_rocket_lake_s_on_lga1200() {
        let id = decode(&i9_11900k());
        assert_eq!(
            codename(id.vendor, id.family, id.model, id.stepping),
            Some(Codename {
                name: "Rocket Lake-S",
                socket: Some("LGA1200")
            })
        );
    }

    #[test]
    fn decodes_a_zen_5_ryzen() {
        let id = decode(&ryzen_9_9950x());
        assert_eq!(
            (id.vendor, id.family, id.model, id.stepping),
            (Amd, 0x1A, 0x44, 0)
        );
        assert_eq!(
            id.instructions,
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI"]
        );
        assert!(!id.hypervisor);
    }

    #[test]
    fn adds_amd_extended_family_and_model() {
        // Ryzen 9 5950X (Zen 3 Vermeer, B0) and Ryzen 5 7600X (Zen 4 Raphael, B2).
        let vermeer = decode(&signature(AMD_ID, 0x00A20F10));
        assert_eq!(
            (vermeer.family, vermeer.model, vermeer.stepping),
            (0x19, 0x21, 0)
        );
        let raphael = decode(&signature(AMD_ID, 0x00A60F12));
        assert_eq!(
            (raphael.family, raphael.model, raphael.stepping),
            (0x19, 0x61, 2)
        );
    }

    #[test]
    fn extended_fields_count_only_for_the_families_that_use_them() {
        // The extended model joins base families 6 and 0xF only: family 5 keeps model 2.
        let family_5 = decode(&signature(INTEL_ID, 0x00010521));
        assert_eq!(
            (family_5.family, family_5.model, family_5.stepping),
            (5, 2, 1)
        );
        // The extended family is added to base family 0xF only.
        let family_6 = decode(&signature(INTEL_ID, 0x0FF00671));
        assert_eq!((family_6.family, family_6.model), (6, 0x07));
        // A Pentium 4 (Prescott, 0F-03-04): base family 0xF with extended family 0.
        let pentium_4 = decode(&signature(INTEL_ID, 0x00000F34));
        assert_eq!(
            (pentium_4.family, pentium_4.model, pentium_4.stepping),
            (0xF, 3, 4)
        );
    }

    #[test]
    fn sse4_2_needs_only_its_cpuid_bit() {
        assert_eq!(sets(ECX_SSE4_2, 0, 0, 0, None), ["SSE4.2"]);
        assert!(sets(0, 0, 0, 0, None).is_empty());
    }

    #[test]
    fn avx_counts_only_when_windows_saves_its_registers() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        let wide = EBX7_AVX2 | EBX7_AVX512F;
        assert_eq!(sets(avx, 0, 0, 0, Some(STATE_AVX)), ["AVX"]);
        // XCR0 without bit 2: the CPU has AVX, AVX2 and AVX-512, but Windows doesn't save the YMM registers.
        assert!(sets(avx, wide, 0, 0, Some(STATE_AVX512 & !(1 << 2))).is_empty());
        assert!(sets(avx, wide, 0, 0, Some(STATE_AVX512 & !(1 << 1))).is_empty());
        // Without OSXSAVE there is no XCR0 to read, and one that is passed anyway isn't trusted.
        assert!(sets(ECX_AVX, wide, 0, 0, None).is_empty());
        assert!(sets(ECX_AVX, wide, 0, 0, Some(STATE_AVX512)).is_empty());
        // XCR0 alone doesn't make a CPU without the AVX bit support AVX.
        assert!(sets(ECX_OSXSAVE, wide, 0, 0, Some(STATE_AVX512)).is_empty());
    }

    #[test]
    fn avx2_needs_usable_avx() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(sets(avx, EBX7_AVX2, 0, 0, Some(STATE_AVX)), ["AVX", "AVX2"]);
        assert!(sets(ECX_OSXSAVE, EBX7_AVX2, 0, 0, Some(STATE_AVX)).is_empty());
    }

    #[test]
    fn avx_512_needs_all_three_of_its_register_states() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(
            sets(avx, EBX7_AVX512F, 0, 0, Some(STATE_AVX512)),
            ["AVX", "AVX-512"]
        );
        for bit in [5, 6, 7] {
            let without = STATE_AVX512 & !(1 << bit);
            assert_eq!(
                sets(avx, EBX7_AVX512F, 0, 0, Some(without)),
                ["AVX"],
                "XCR0 bit {bit}"
            );
        }
        assert_eq!(sets(avx, 0, 0, 0, Some(STATE_AVX512)), ["AVX"]);
    }

    #[test]
    fn avx_vnni_reads_leaf_7_sub_leaf_1_and_needs_avx() {
        let avx = ECX_OSXSAVE | ECX_AVX;
        assert_eq!(
            sets(avx, 0, 0, EAX7_1_AVX_VNNI, Some(STATE_AVX)),
            ["AVX", "AVX-VNNI"]
        );
        assert!(sets(ECX_OSXSAVE, 0, 0, EAX7_1_AVX_VNNI, Some(STATE_AVX)).is_empty());
    }

    #[test]
    fn amx_needs_the_tile_config_and_tile_data_state() {
        assert_eq!(
            sets(ECX_OSXSAVE, 0, EDX7_AMX_TILE, 0, Some(STATE_AMX)),
            ["AMX"]
        );
        for xcr0 in [Some(1 << 17), Some(1 << 18), Some(STATE_AVX512), None] {
            assert!(
                sets(ECX_OSXSAVE, 0, EDX7_AMX_TILE, 0, xcr0).is_empty(),
                "{xcr0:?}"
            );
        }
        assert!(sets(ECX_OSXSAVE, 0, 0, 0, Some(STATE_AMX)).is_empty());
    }

    #[test]
    fn every_set_is_listed_in_order() {
        let ecx = ECX_SSE4_2 | ECX_OSXSAVE | ECX_AVX;
        let ebx7 = EBX7_AVX2 | EBX7_AVX512F;
        assert_eq!(
            sets(
                ecx,
                ebx7,
                EDX7_AMX_TILE,
                EAX7_1_AVX_VNNI,
                Some(STATE_AVX512 | STATE_AMX)
            ),
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX"]
        );
    }

    #[test]
    fn the_hypervisor_bit_is_leaf_1_ecx_bit_31() {
        let mut raw = i9_11900k();
        assert!(!decode(&raw).hypervisor);
        raw.leaf1[2] |= ECX_HYPERVISOR;
        assert!(decode(&raw).hypervisor);
    }

    #[test]
    fn leaf_0x16_clocks_are_intel_only_and_never_zero() {
        let clocks = |raw: &CpuidRaw| {
            let id = decode(raw);
            (id.base_mhz, id.max_mhz)
        };
        let mut intel = signature(INTEL_ID, 0x000A0671);
        // Only bits 15:0 hold the clock.
        intel.leaf16 = Some([0xFFFF_0000 | 3500, 0x0001_0000 | 5300, 100]);
        assert_eq!(clocks(&intel), (Some(3500), Some(5300)));
        // Zeros, as some virtual machines report.
        intel.leaf16 = Some([0, 0, 0]);
        assert_eq!(clocks(&intel), (None, None));
        intel.leaf16 = None;
        assert_eq!(clocks(&intel), (None, None));
        // AMD keeps leaf 0x16 reserved: whatever it holds is ignored.
        let mut amd = signature(AMD_ID, 0x00A60F12);
        amd.leaf16 = Some([3500, 5300, 100]);
        assert_eq!(clocks(&amd), (None, None));
        assert_eq!(clocks(&ryzen_9_9950x()), (None, None));
    }

    #[test]
    fn other_vendors_are_other_but_still_decoded() {
        // Hygon, VIA, Zhaoxin and nothing at all.
        for vendor in ["HygonGenuine", "CentaurHauls", "  Shanghai  ", ""] {
            let id = decode(&signature(vendor, 0x00A20F10));
            assert_eq!(id.vendor, Other, "{vendor:?}");
            assert_eq!((id.family, id.model), (0x19, 0x21));
        }
        assert_eq!(decode(&CpuidRaw::default()), CpuIdentity::default());
    }

    /// A real chip for every Intel row: leaf 1 EAX, codename, socket. Signatures from InstLatx64's dump names and
    /// Intel's microcode release notes.
    const INTEL_CHIPS: &[(u32, &str, Option<&str>)] = &[
        (0x000506E3, "Skylake-S/H", Some("LGA1151")), // Core i7-6700K
        (0x000406E3, "Skylake-U/Y", None),            // Core i7-6500U
        (0x000906E9, "Kaby Lake-S/H", Some("LGA1151")), // Core i3-7100, i7-8705G
        (0x000806E9, "Kaby Lake-U/Y / Amber Lake-Y", None), // Core i5-7200U, m3-8100Y
        (0x000806EA, "Kaby Lake-R / Coffee Lake-U", None), // Core i5-8250U
        (0x000806EB, "Whiskey Lake-U", None),         // Core i7-8565U
        (
            0x000806EC,
            "Whiskey Lake-U / Amber Lake-Y / Comet Lake-U",
            None,
        ), // Core i5-10210U
        (0x000906EA, "Coffee Lake-S/H", Some("LGA1151")), // Core i7-8700K, i7-9750H
        (0x000906EC, "Coffee Lake-S/H", Some("LGA1151")), // Core i5-9600K
        (0x000A0655, "Comet Lake-S", Some("LGA1200")), // Core i9-10900K
        (0x000A0654, "Comet Lake-S", Some("LGA1200")), // Xeon W-1290P
        (0x000A0652, "Comet Lake-H", None),           // Core i7-10750H
        (0x000A0660, "Comet Lake-U", None),           // Core i7-10710U
        (0x00060663, "Cannon Lake-U", None),          // Core i3-8121U
        (0x000706E5, "Ice Lake-U/Y", None),           // Core i7-1065G7
        (0x000806A1, "Lakefield", None),              // Core i5-L16G7
        (0x000806C1, "Tiger Lake-UP3/UP4", None),     // Core i5-1135G7
        (0x000806D1, "Tiger Lake-H", None),           // Core i7-11800H
        (0x000A0671, "Rocket Lake-S", Some("LGA1200")), // Core i9-11900K
        (0x00090672, "Alder Lake-S/HX", Some("LGA1700")), // Core i9-12900K, i5-12400
        (0x000906A3, "Alder Lake-H/P/U", None),       // Core i9-12900H
        (0x000B06E0, "Alder Lake-N / Twin Lake", None), // N95, Core 3 N355
        (0x000B0671, "Raptor Lake-S/HX", Some("LGA1700")), // Core i9-13900K
        (0x000B0674, "Bartlett Lake-S", Some("LGA1700")), // Core 7 251TE
        (0x000B06A2, "Raptor Lake-H/P/U", None),      // Core i5-1350P
        (0x000B06F2, "Raptor Lake-S/HX", Some("LGA1700")), // Core i5-13500
        (0x000D0670, "Bartlett Lake-S", Some("LGA1700")), // Core 9 273PQE
        (0x000A06A4, "Meteor Lake-H/U", None),        // Core Ultra 5 125H
        (0x000C0662, "Arrow Lake-S/HX", Some("LGA1851")), // Core Ultra 5 245K, 9 275HX
        (0x000C0652, "Arrow Lake-H", None),           // Core Ultra 9 285H
        (0x000B0650, "Arrow Lake-U", None),           // Core Ultra 7 265U
        (0x000B06D1, "Lunar Lake", None),             // Core Ultra 9 288V
        (0x000C06C3, "Panther Lake", None),           // Core Ultra 7 355
        (0x000D0651, "Wildcat Lake", None),           // Core 7 350
    ];

    /// A real chip for every AMD row. Signatures from InstLatx64's dump names; Chagall's from Linux's amd.c, the
    /// 8500G's and 9955HX's from CPU-Z validations.
    const AMD_CHIPS: &[(u32, &str, Option<&str>)] = &[
        (0x00800F11, "Summit Ridge", Some("AM4")), // Ryzen 7 1800X
        (0x00800F82, "Pinnacle Ridge", Some("AM4")), // Ryzen 5 2600X
        (0x00810F10, "Raven Ridge", Some("AM4")),  // Ryzen 5 2500U
        (0x00810F81, "Picasso", Some("AM4")),      // Athlon PRO 300GE, Ryzen 5 3550H
        (0x00820F01, "Dali", None),                // AMD 3020e
        (0x00830F10, "Castle Peak", Some("sTRX4")), // Threadripper 3970X
        (0x00860F01, "Renoir", Some("AM4")),       // Ryzen 7 4800H
        (0x00860F81, "Lucienne", None),            // Ryzen 5 5500U
        (0x00870F10, "Matisse", Some("AM4")),      // Ryzen 5 3600X
        (0x00890F02, "Van Gogh", None),            // Steam Deck LCD (Custom APU 0405)
        (0x00890F10, "Van Gogh", None),            // Steam Deck OLED (Custom APU 0932)
        (0x008A0F00, "Mendocino", None),           // Ryzen 5 7520U
        (0x00A00F82, "Chagall", Some("sWRX8")),    // Threadripper PRO 5000WX
        (0x00A10F81, "Storm Peak", Some("sTR5")),  // Threadripper PRO 7995WX
        (0x00A20F12, "Vermeer", Some("AM4")),      // Ryzen 7 5700X3D
        (0x00A40F41, "Rembrandt", None),           // Ryzen 7 6800HS
        (0x00A50F00, "Cezanne / Barcelo", Some("AM4")), // Ryzen 7 5800H, 5825U
        (0x00A60F12, "Raphael / Dragon Range", Some("AM5")), // Ryzen 5 7600X
        (0x00A70F41, "Phoenix", None),             // Ryzen 9 7940H
        (0x00A70F52, "Phoenix / Hawk Point", Some("AM5")), // Ryzen 7 8700G, 8845HS
        (0x00A70F80, "Phoenix 2", Some("AM5")),    // Ryzen 5 8500G, Ryzen Z1
        (0x00A70FC0, "Hawk Point 2", None),        // Ryzen 5 PRO 215
        (0x00B00F81, "Shimada Peak", Some("sTR5")), // Threadripper PRO 9985WX
        (0x00B20F40, "Strix Point / Gorgon Point", None), // Ryzen AI 9 HX 370, AI 9 465
        (0x00B40F40, "Granite Ridge / Fire Range", Some("AM5")), // Ryzen 9 9950X, 9955HX
        (0x00B60F00, "Krackan Point / Gorgon Point", None), // Ryzen AI 7 PRO 350, AI 7 450
        (0x00B70F00, "Strix Halo", None),          // Ryzen AI MAX+ 395
    ];

    #[test]
    fn every_chip_resolves_to_its_codename_and_socket() {
        for (vendor, chips) in [(INTEL_ID, INTEL_CHIPS), (AMD_ID, AMD_CHIPS)] {
            for &(eax, name, socket) in chips {
                let id = decode(&signature(vendor, eax));
                assert_eq!(
                    codename(id.vendor, id.family, id.model, id.stepping),
                    Some(Codename { name, socket }),
                    "{eax:#010X}"
                );
            }
        }
    }

    #[test]
    fn every_row_is_reachable_and_has_a_chip() {
        let tables = [
            (Intel, INTEL, INTEL_ID, INTEL_CHIPS),
            (Amd, AMD, AMD_ID, AMD_CHIPS),
        ];
        for (vendor, rows, id, chips) in tables {
            for row in rows {
                // No earlier row hides any of this row's steppings.
                let expected = Some(Codename {
                    name: row.name,
                    socket: row.socket,
                });
                for stepping in row.steppings.clone() {
                    let found = codename(vendor, row.family, row.model, stepping);
                    assert_eq!(found, expected, "{} stepping {stepping}", row.name);
                }
                let has_chip = chips.iter().any(|&(eax, _, _)| {
                    let chip = decode(&signature(id, eax));
                    chip.family == row.family
                        && chip.model == row.model
                        && row.steppings.contains(&chip.stepping)
                });
                assert!(has_chip, "no chip for {}", row.name);
            }
        }
    }

    #[test]
    fn kaby_lake_and_coffee_lake_share_model_0x9e() {
        let at = |stepping| codename(Intel, 6, 0x9E, stepping).map(|c| c.name);
        assert_eq!(at(9), Some("Kaby Lake-S/H"));
        for stepping in 10..=13 {
            assert_eq!(at(stepping), Some("Coffee Lake-S/H"));
        }
        assert_eq!(at(8), None);
        assert_eq!(at(14), None);
    }

    #[test]
    fn model_0x8e_tells_four_laptop_generations_apart_by_stepping() {
        let at = |stepping| codename(Intel, 6, 0x8E, stepping);
        assert_eq!(at(9).map(|c| c.name), Some("Kaby Lake-U/Y / Amber Lake-Y"));
        assert_eq!(at(10).map(|c| c.name), Some("Kaby Lake-R / Coffee Lake-U"));
        assert_eq!(at(11).map(|c| c.name), Some("Whiskey Lake-U"));
        assert_eq!(
            at(12).map(|c| c.name),
            Some("Whiskey Lake-U / Amber Lake-Y / Comet Lake-U")
        );
        assert!((9..=12).all(|stepping| at(stepping).is_some_and(|c| c.socket.is_none())));
        assert_eq!(at(8), None);
        assert_eq!(at(13), None);
    }

    #[test]
    fn comet_lake_h_and_s_differ_by_stepping() {
        let h = Codename {
            name: "Comet Lake-H",
            socket: None,
        };
        let s = Codename {
            name: "Comet Lake-S",
            socket: Some("LGA1200"),
        };
        assert_eq!(codename(Intel, 6, 0xA5, 2), Some(h));
        for stepping in 3..=5 {
            assert_eq!(codename(Intel, 6, 0xA5, stepping), Some(s));
        }
        assert_eq!(codename(Intel, 6, 0xA5, 1), None);
    }

    #[test]
    fn bartlett_lake_shares_raptor_lakes_model() {
        let at = |stepping| codename(Intel, 6, 0xB7, stepping).map(|c| c.name);
        assert_eq!(at(1), Some("Raptor Lake-S/HX"));
        assert_eq!(at(4), Some("Bartlett Lake-S"));
        assert_eq!(at(2), None);
    }

    #[test]
    fn unknown_models_are_not_guessed() {
        for (vendor, family, model, stepping) in [
            (Intel, 6, 0x55, 4),    // Skylake-SP Xeon
            (Intel, 6, 0x8F, 8),    // Sapphire Rapids Xeon w7-2475X
            (Intel, 6, 0x3C, 3),    // Haswell, older than the table
            (Intel, 0x12, 0x01, 0), // Nova Lake: no shipping chip to confirm yet
            (Amd, 0x19, 0x01, 1),   // EPYC Milan
            (Amd, 0x1A, 0x68, 0),   // Ryzen AI 5 430: one dump, not confirmed
            (Amd, 0x15, 0x02, 0),   // FX (Piledriver), older than the table
        ] {
            assert_eq!(
                codename(vendor, family, model, stepping),
                None,
                "{vendor:?} {family:#X}/{model:#X}"
            );
        }
    }

    #[test]
    fn the_vendor_must_match() {
        assert_eq!(codename(Amd, 6, 0xA7, 1), None);
        assert_eq!(codename(Intel, 0x19, 0x21, 0), None);
        assert_eq!(codename(Other, 6, 0xA7, 1), None);
        assert_eq!(codename(Other, 0x19, 0x21, 0), None);
    }

    #[test]
    fn decodes_amx_on_a_xeon_w_that_the_table_does_not_name() {
        // Xeon w7-2475X (Sapphire Rapids) from InstLatx64's GenuineIntel00806F8_SapphireRapids_05, with Windows'
        // AMX state on.
        let raw = CpuidRaw {
            vendor: INTEL_ID.into(),
            max_leaf: 0x20,
            leaf1: [0x000806F8, 0x00800800, 0x7FFEFBFF, 0xBFEBFBFF],
            leaf7: [0x00000002, 0xF3BFBFFB, 0xBB417FEE, 0xFFDD4430],
            leaf7_1_eax: 0x00001C30,
            leaf16: None,
            brand: "Intel(R) Xeon(R) w7-2475X".into(),
            xcr0: Some(STATE_AVX512 | STATE_AMX),
        };
        let id = decode(&raw);
        assert_eq!(
            id.instructions,
            ["SSE4.2", "AVX", "AVX2", "AVX-512", "AVX-VNNI", "AMX"]
        );
        assert_eq!(codename(id.vendor, id.family, id.model, id.stepping), None);
    }
}
