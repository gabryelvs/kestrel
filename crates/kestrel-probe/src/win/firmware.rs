//! IOMMU: firmware publishes an ACPI DMAR table (Intel VT-d) or IVRS table (AMD-Vi) only when it's enabled.
use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;

pub fn iommu_enabled() -> bool {
    let acpi = u32::from_be_bytes(*b"ACPI");
    [*b"DMAR", *b"IVRS"].iter().any(|signature| {
        // SAFETY: a null buffer of size 0 only asks for the table's size; nothing is written.
        unsafe {
            GetSystemFirmwareTable(
                acpi,
                u32::from_le_bytes(*signature),
                std::ptr::null_mut(),
                0,
            ) > 0
        }
    })
}
