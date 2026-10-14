//! The processor and two firmware facts read with it: CPUID (identity, instruction sets, a running hypervisor), the
//! cores and caches (`GetLogicalProcessorInformationEx`), the base clock when CPUID has none (PDH), the boot mode
//! (`GetFirmwareType`) and whether virtualization is on. All user mode: CPUID and XGETBV are ordinary instructions
//! that only read the processor's identification, and the rest only ask Windows.
use super::{pdh, problem};
use crate::parse;
use kestrel_core::cpu::{self, CpuidRaw};
use kestrel_core::topology::{self, PhysicalCore};
use kestrel_core::{CacheKind, ProbeError, SystemSnapshot};
use windows_sys::Win32::System::SystemInformation::{
    CacheData, CacheInstruction, CacheUnified, FirmwareTypeUnknown, GetFirmwareType,
    GetLogicalProcessorInformationEx, RelationAll, RelationCache, RelationProcessorCore,
    CACHE_RELATIONSHIP, FIRMWARE_TYPE, GROUP_AFFINITY, PROCESSOR_RELATIONSHIP,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{IsProcessorFeaturePresent, PF_VIRT_FIRMWARE_ENABLED};

// `parse::logical_processors` reads the records byte by byte at these offsets (64-bit Windows); this keeps them true.
const _: () = {
    use std::mem::offset_of;
    assert!(offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous) == 8);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, EfficiencyClass) == 1);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, GroupCount) == 22);
    assert!(offset_of!(PROCESSOR_RELATIONSHIP, GroupMask) == 24);
    assert!(size_of::<GROUP_AFFINITY>() == 16 && offset_of!(GROUP_AFFINITY, Group) == 8);
    assert!(offset_of!(CACHE_RELATIONSHIP, CacheSize) == 4);
    assert!(offset_of!(CACHE_RELATIONSHIP, Type) == 8);
    assert!(RelationProcessorCore == 0 && RelationCache == 2);
    assert!(CacheUnified == 0 && CacheInstruction == 1 && CacheData == 2);
};

/// Fills the processor's identity, cores, caches, clocks and virtualization, and the board's boot mode. Runs after
/// WMI: a laptop (the machine query) gets no socket, and virtualization counts VBS (the device-guard query). WMI stays
/// primary for the name, vendor and core and thread counts; the processor's own answers fill in only what WMI left
/// empty (its query failed, or it said nothing).
pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    let mut hypervisor = false;
    match read_cpuid() {
        Some(raw) => {
            let id = cpu::decode(&raw);
            hypervisor = id.hypervisor;
            parse::fill_missing_cpu_identity(&mut s.cpu, &raw.brand, id.vendor);
            if let Some(codename) = cpu::codename(id.vendor, id.family, id.model, id.stepping) {
                s.cpu.codename = Some(codename.name.to_string());
                s.cpu.socket = parse::cpu_socket(s.machine.form_factor, codename.socket);
            }
            s.cpu.family = Some(id.family);
            s.cpu.model = Some(id.model);
            s.cpu.stepping = Some(id.stepping);
            s.cpu.base_mhz = id.base_mhz;
            s.cpu.max_mhz = id.max_mhz;
            s.cpu.instructions = id.instructions;
        }
        None => errors.push(problem("cpu-id", "CPUID is read only on x86-64 processors")),
    }
    match topology() {
        Some((cores, caches)) => {
            parse::fill_missing_cpu_counts(&mut s.cpu, &cores);
            if let Some((p_cores, e_cores)) = topology::p_e_split(&cores) {
                s.cpu.p_cores = Some(p_cores);
                s.cpu.e_cores = Some(e_cores);
            }
            s.cpu.caches = topology::group_caches(&caches);
        }
        None => errors.push(problem(
            "topology",
            "Windows didn't list the processor's cores and caches (GetLogicalProcessorInformationEx)",
        )),
    }
    // CPUID gives the base clock only on Intel (leaf 0x16); otherwise Windows' rated frequency stands in.
    if s.cpu.base_mhz.is_none() {
        s.cpu.base_mhz = rated_mhz();
    }
    match firmware_type() {
        Some(firmware) => s.board.boot_mode = parse::boot_mode(firmware),
        None => errors.push(problem(
            "firmware",
            "couldn't read the firmware type (GetFirmwareType)",
        )),
    }
    // SAFETY: only asks whether a processor feature is on; nothing is passed or written.
    let firmware_flag = unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) } != 0;
    s.cpu.virtualization =
        parse::virtualization(hypervisor, s.security.vbs_running, Some(firmware_flag));
}

/// The CPUID leaves `kestrel_core::cpu::decode` reads.
#[cfg(target_arch = "x86_64")]
fn read_cpuid() -> Option<CpuidRaw> {
    use std::arch::x86_64::{__cpuid, __cpuid_count, CpuidResult};
    fn registers(r: CpuidResult) -> [u32; 4] {
        [r.eax, r.ebx, r.ecx, r.edx]
    }
    let leaf0 = __cpuid(0);
    let max_leaf = leaf0.eax;
    let leaf1 = if max_leaf >= 1 {
        registers(__cpuid(1))
    } else {
        [0; 4]
    };
    let leaf7 = if max_leaf >= 7 {
        registers(__cpuid_count(7, 0))
    } else {
        [0; 4]
    };
    let leaf7_1_eax = if max_leaf >= 7 && leaf7[0] >= 1 {
        __cpuid_count(7, 1).eax
    } else {
        0
    };
    let leaf16 = (max_leaf >= 0x16).then(|| {
        let r = __cpuid(0x16);
        [r.eax, r.ebx, r.ecx]
    });
    let brand = if __cpuid(0x8000_0000).eax >= 0x8000_0004 {
        let parts: Vec<u32> = (0x8000_0002..=0x8000_0004u32)
            .flat_map(|leaf| registers(__cpuid(leaf)))
            .collect();
        parse::cpuid_text(&parts)
    } else {
        String::new()
    };
    // XGETBV exists only once the OS has turned XSAVE on, which leaf 1 reports as OSXSAVE (ECX bit 27).
    let osxsave = leaf1[2] & (1 << 27) != 0;
    // SAFETY: `xcr0` needs XSAVE, and OSXSAVE set means the processor has it and the OS turned it on.
    let xcr0 = osxsave.then(|| unsafe { xcr0() });
    Some(CpuidRaw {
        vendor: parse::cpuid_text(&[leaf0.ebx, leaf0.edx, leaf0.ecx]),
        max_leaf,
        leaf1,
        leaf7,
        leaf7_1_eax,
        leaf16,
        brand,
        xcr0,
    })
}

/// Off x86-64 there is no CPUID to read.
#[cfg(not(target_arch = "x86_64"))]
fn read_cpuid() -> Option<CpuidRaw> {
    None
}

/// XCR0: which register sets (SSE, AVX, AVX-512, AMX) the OS saves when it switches threads. Compiled with XSAVE
/// enabled, so it may only be called where the processor has XSAVE and the OS turned it on (OSXSAVE).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "xsave")]
fn xcr0() -> u64 {
    // SAFETY: XGETBV with ECX = 0 only reads XCR0, and this function runs only where XGETBV exists (see above).
    unsafe { std::arch::x86_64::_xgetbv(0) }
}

/// The physical cores, and every cache as (level, kind, size in KB).
type Topology = (Vec<PhysicalCore>, Vec<(u8, CacheKind, u32)>);

/// The physical cores and every cache, from `GetLogicalProcessorInformationEx(RelationAll)`. None when Windows doesn't
/// answer or lists no core. The live clocks use the cores too.
pub(crate) fn topology() -> Option<Topology> {
    let mut needed = 0u32;
    // SAFETY: a null buffer with a length of 0 only asks how many bytes the answer needs; nothing is written.
    unsafe { GetLogicalProcessorInformationEx(RelationAll, std::ptr::null_mut(), &mut needed) };
    // A processor added between two calls makes the answer outgrow the buffer; Windows then says the new size.
    for _ in 0..3 {
        if needed == 0 {
            return None;
        }
        // u64s, so the records (which hold 8-byte affinity masks) are aligned as Windows expects.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        let room = u32::try_from(size_of_val(buffer.as_slice())).ok()?;
        let mut length = room;
        // SAFETY: `buffer` is 8-byte aligned and `room` bytes long, which is what `length` tells Windows.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(RelationAll, buffer.as_mut_ptr().cast(), &mut length)
        } != 0;
        if ok {
            let bytes: Vec<u8> = buffer
                .iter()
                .flat_map(|word| word.to_ne_bytes())
                .take(length as usize)
                .collect();
            let (cores, caches) = parse::logical_processors(&bytes);
            return (!cores.is_empty()).then_some((cores, caches));
        }
        if length <= room {
            return None; // it failed for another reason than room
        }
        needed = length;
    }
    None
}

/// The processor's rated clock in MHz from PDH's `Processor Frequency`, which Windows keeps at the rated (base)
/// clock; the live clock is that times `% Processor Performance`. It isn't a rate, so one sample is enough.
fn rated_mhz() -> Option<u32> {
    let query = pdh::Query::open(r"\Processor Information(_Total)\Processor Frequency")?;
    if !query.collect() {
        return None;
    }
    let (_, mhz) = query.formatted_array(0)?.into_iter().next()?;
    (mhz >= 1.0).then(|| mhz.round() as u32)
}

fn firmware_type() -> Option<FIRMWARE_TYPE> {
    let mut firmware: FIRMWARE_TYPE = FirmwareTypeUnknown;
    // SAFETY: `firmware` is a valid place for the answer.
    (unsafe { GetFirmwareType(&mut firmware) } != 0).then_some(firmware)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_lists_each_logical_processor_once() {
        let (cores, caches) = topology().expect("Windows lists the processor's cores");
        let mut logical: Vec<(u16, u8)> = cores.iter().flat_map(|c| c.logical.clone()).collect();
        let count = logical.len();
        logical.sort_unstable();
        logical.dedup();
        assert_eq!(logical.len(), count, "{cores:?}");
        assert!(!caches.is_empty());
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn cpuid_names_the_vendor() {
        let raw = read_cpuid().expect("x86-64 processors have CPUID");
        assert!(raw.max_leaf >= 1, "{raw:?}");
        assert!(!raw.vendor.is_empty(), "{raw:?}");
    }
}
