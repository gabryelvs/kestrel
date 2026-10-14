//! Configuration Manager reads: the graphics card's PCIe link (its PCI device and the bridges above it), the memory
//! ranges Windows gave the card (Resizable BAR), and the Intel LPC/eSPI controller's name (the chipset). Read-only:
//! every call reads a device property or a list Windows keeps; nothing is changed and no driver is involved.
use super::problem;
use crate::parse;
use kestrel_core::pcie::{self, PciNode};
use kestrel_core::{hardware, PcieLink, ProbeError, SystemSnapshot};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Free_Log_Conf_Handle, CM_Free_Res_Des_Handle, CM_Get_DevNode_PropertyW, CM_Get_Device_IDW,
    CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_Get_First_Log_Conf,
    CM_Get_Next_Res_Des, CM_Get_Parent, CM_Get_Res_Des_Data, CM_Get_Res_Des_Data_Size,
    CM_Locate_DevNodeW, ResType_Mem, ResType_MemLarge, ALLOC_LOG_CONF,
    CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL,
    CM_RESTYPE, CR_BUFFER_SMALL, CR_NO_MORE_LOG_CONF, CR_SUCCESS, MAX_DEVICE_ID_LEN, MEM_DES,
    MEM_LARGE_DES,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_DeviceDesc, DEVPROPTYPE, DEVPROP_TYPE_BYTE, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_UINT32,
};
use windows_sys::Win32::Foundation::DEVPROPKEY;

/// The PCI device property set (pciprop.h). windows-sys doesn't export its keys, so they are declared here.
const PCI_DEVICE: GUID = GUID::from_u128(0x3ab22e31_8264_4b4e_9af5_a8d2d8e33e62);

const fn pci_key(pid: u32) -> DEVPROPKEY {
    DEVPROPKEY {
        fmtid: PCI_DEVICE,
        pid,
    }
}

/// UINT32: what kind of PCI device or bridge this is (`pcie::ROOT_PORT` and the switch ports among them).
const DEVICE_TYPE: DEVPROPKEY = pci_key(1);
/// BYTE: the class code's base class and subclass (06/01 is an ISA bridge, which is how LPC/eSPI shows).
const BASE_CLASS: DEVPROPKEY = pci_key(3);
const SUB_CLASS: DEVPROPKEY = pci_key(4);
/// UINT32: link speed codes (1–6 = PCIe 1.0–6.0) and widths (lanes), now and at most.
const CURRENT_LINK_SPEED: DEVPROPKEY = pci_key(9);
const CURRENT_LINK_WIDTH: DEVPROPKEY = pci_key(10);
const MAX_LINK_SPEED: DEVPROPKEY = pci_key(11);
const MAX_LINK_WIDTH: DEVPROPKEY = pci_key(12);

/// The walk reads the card and at most this many PCI devices above it (a switch inside the card is two of them).
const MAX_ANCESTORS: usize = 8;

// The pure decoders in `parse` use their own copies of these numbers and offsets; this keeps them true.
const _: () = {
    use std::mem::offset_of;
    assert!(parse::DEVPROP_BYTE == DEVPROP_TYPE_BYTE);
    assert!(parse::DEVPROP_UINT32 == DEVPROP_TYPE_UINT32);
    assert!(parse::DEVPROP_STRING == DEVPROP_TYPE_STRING);
    assert!(offset_of!(MEM_DES, MD_Alloc_Base) == 8 && offset_of!(MEM_DES, MD_Alloc_End) == 16);
    assert!(
        offset_of!(MEM_LARGE_DES, MLD_Alloc_Base) == 8
            && offset_of!(MEM_LARGE_DES, MLD_Alloc_End) == 16
    );
};

/// Each graphics card's PCIe link and Resizable BAR, and the board's chipset. Runs after WMI, which lists the cards
/// with their PNPDeviceID and reads the board's name.
pub fn fill(s: &mut SystemSnapshot, errors: &mut Vec<ProbeError>) {
    for gpu in &mut s.gpus {
        // Virtual adapters (ROOT\…, SWD\…, VMBUS\…) and cards WMI gave no id for have no PCI link to read.
        if !parse::is_pci_instance(&gpu.pnp_device_id) {
            continue;
        }
        let Some(node) = locate(&gpu.pnp_device_id) else {
            errors.push(problem(
                "pcie",
                format!("couldn't find {} among Windows' devices", gpu.name),
            ));
            continue;
        };
        gpu.pcie = pcie::resolve(&chain(node, &gpu.pnp_device_id), &gpu.pnp_device_id);
        match memory_ranges(node) {
            Some(ranges) => (gpu.largest_bar_mb, gpu.resizable_bar) = pcie::bar_summary(&ranges),
            None => errors.push(problem(
                "pcie",
                format!("couldn't read the memory ranges Windows gave {}", gpu.name),
            )),
        }
    }
    let description = lpc_description().unwrap_or_else(|e| {
        errors.push(problem("chipset", e));
        None
    });
    // The board's name is enough on its own, so the chipset is worked out even without the controller's name.
    s.board.chipset = hardware::chipset(&s.board.product, description.as_deref());
}

/// The current link of the card's slot device. None when the card isn't a PCI device Windows lists, or its link
/// isn't reported (a card asleep, a virtual machine).
pub fn gpu_link(pnp_device_id: &str) -> Option<PcieLink> {
    if !parse::is_pci_instance(pnp_device_id) {
        return None;
    }
    let nodes = chain(locate(pnp_device_id)?, pnp_device_id);
    pcie::slot_device(&nodes, pnp_device_id)?.current.clone()
}

/// The card's node and its PCI ancestors up to the first root port (at most MAX_ANCESTORS), each with its type and
/// links. No node has an empty instance id and none names an empty parent: `card_id` is a PCI instance id (both
/// callers check, and the ancestors are followed only while they are), and `device_id` never returns an empty one.
fn chain(card: u32, card_id: &str) -> Vec<PciNode> {
    let mut nodes = Vec::new();
    let mut next = Some((card, card_id.to_string()));
    while let Some((node, instance_id)) = next.take() {
        let parent = parent_of(node);
        let device_type = property_u32(node, &DEVICE_TYPE);
        nodes.push(PciNode {
            instance_id,
            parent: parent.as_ref().map(|(_, id)| id.clone()),
            device_type,
            current: pcie::link_from_codes(
                property_u32(node, &CURRENT_LINK_SPEED),
                property_u32(node, &CURRENT_LINK_WIDTH),
            ),
            max: pcie::link_from_codes(
                property_u32(node, &MAX_LINK_SPEED),
                property_u32(node, &MAX_LINK_WIDTH),
            ),
        });
        if device_type == Some(pcie::ROOT_PORT) || nodes.len() > MAX_ANCESTORS {
            break;
        }
        next = parent.filter(|(_, id)| parse::is_pci_instance(id));
    }
    nodes
}

/// The device node of a device instance id, if that device is present.
fn locate(instance_id: &str) -> Option<u32> {
    let id = wide(instance_id);
    let mut node = 0u32;
    // SAFETY: `id` is NUL-terminated and outlives the call; `node` is a valid place for the handle.
    let status = unsafe { CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    (status == CR_SUCCESS).then_some(node)
}

/// The parent device's node and instance id.
fn parent_of(node: u32) -> Option<(u32, String)> {
    let mut parent = 0u32;
    // SAFETY: `parent` is a valid place for the handle.
    if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
        return None;
    }
    Some((parent, device_id(parent)?))
}

fn device_id(node: u32) -> Option<String> {
    // An instance id is at most MAX_DEVICE_ID_LEN characters, plus its NUL.
    let mut buffer = [0u16; MAX_DEVICE_ID_LEN as usize + 1];
    let room = buffer.len() as u32;
    // SAFETY: `buffer` is writable for the `room` UTF-16 units passed with it.
    let status = unsafe { CM_Get_Device_IDW(node, buffer.as_mut_ptr(), room, 0) };
    if status != CR_SUCCESS {
        return None;
    }
    Some(parse::exe_name(&buffer)).filter(|id| !id.is_empty())
}

/// A device property's type and bytes, if the device has it and it fits in `room` bytes.
fn property(node: u32, key: &DEVPROPKEY, room: u32) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut property_type: DEVPROPTYPE = 0;
    let mut data = vec![0u8; room as usize];
    let mut size = room;
    // SAFETY: `key` and `property_type` are valid, and `data` is writable for the `size` bytes passed with it.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut property_type,
            data.as_mut_ptr(),
            &mut size,
            0,
        )
    };
    if status != CR_SUCCESS {
        return None;
    }
    data.truncate(size as usize);
    Some((property_type, data))
}

fn property_u32(node: u32, key: &DEVPROPKEY) -> Option<u32> {
    let (property_type, data) = property(node, key, 4)?;
    parse::device_property_u32(property_type, &data)
}

fn property_byte(node: u32, key: &DEVPROPKEY) -> Option<u8> {
    let (property_type, data) = property(node, key, 1)?;
    parse::device_property_byte(property_type, &data)
}

fn property_text(node: u32, key: &DEVPROPKEY) -> Option<String> {
    let mut property_type: DEVPROPTYPE = 0;
    let mut size = 0u32;
    // SAFETY: a null buffer with a size of 0 only asks how many bytes the value needs; nothing is written.
    let status = unsafe {
        CM_Get_DevNode_PropertyW(
            node,
            key,
            &mut property_type,
            std::ptr::null_mut(),
            &mut size,
            0,
        )
    };
    if status != CR_BUFFER_SMALL {
        return None;
    }
    let (property_type, data) = property(node, key, size)?;
    parse::device_property_text(property_type, &data)
}

/// The size in bytes of every memory range Windows allocated to the device (its allocated configuration's memory and
/// large-memory descriptors). Empty when it has none; None when they can't be read.
fn memory_ranges(node: u32) -> Option<Vec<u64>> {
    let mut config = 0usize;
    // SAFETY: `config` is a valid place for the configuration's handle.
    match unsafe { CM_Get_First_Log_Conf(&mut config, node, ALLOC_LOG_CONF) } {
        CR_SUCCESS => {}
        CR_NO_MORE_LOG_CONF => return Some(Vec::new()), // nothing allocated
        _ => return None,
    }
    let mut ranges = descriptor_sizes(config, ResType_Mem);
    ranges.extend(descriptor_sizes(config, ResType_MemLarge));
    // SAFETY: `config` came from CM_Get_First_Log_Conf above and is freed once.
    unsafe { CM_Free_Log_Conf_Handle(config) };
    Some(ranges)
}

/// The allocated size of each descriptor of resource type `kind` in the configuration `config`.
fn descriptor_sizes(config: usize, kind: CM_RESTYPE) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut last: Option<usize> = None;
    loop {
        let mut next = 0usize;
        // SAFETY: the walk starts at the open configuration and goes on from the descriptor read last, which is still
        // open; `next` is a valid place for the next handle, and the resource type isn't asked for (null).
        let status = unsafe {
            CM_Get_Next_Res_Des(
                &mut next,
                last.unwrap_or(config),
                kind,
                std::ptr::null_mut(),
                0,
            )
        };
        if let Some(done) = last.take() {
            // SAFETY: `done` came from CM_Get_Next_Res_Des, the call above was its last use, and it is freed once.
            unsafe { CM_Free_Res_Des_Handle(done) };
        }
        if status != CR_SUCCESS {
            return sizes;
        }
        sizes.extend(
            descriptor_data(next)
                .as_deref()
                .and_then(parse::allocated_range_bytes),
        );
        last = Some(next);
    }
}

fn descriptor_data(descriptor: usize) -> Option<Vec<u8>> {
    let mut size = 0u32;
    // SAFETY: `size` is a valid place for the answer.
    if unsafe { CM_Get_Res_Des_Data_Size(&mut size, descriptor, 0) } != CR_SUCCESS {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` is writable for the `size` bytes passed with it.
    let status = unsafe { CM_Get_Res_Des_Data(descriptor, data.as_mut_ptr().cast(), size, 0) };
    (status == CR_SUCCESS).then_some(data)
}

/// The name of the Intel LPC/eSPI controller (vendor 8086, PCI class 06/01), which names the chipset on Intel boards.
/// Ok(None) on boards without one (AMD); Err when Windows can't list its PCI devices.
fn lpc_description() -> Result<Option<String>, &'static str> {
    let ids =
        present_pci_devices().ok_or("couldn't list the PCI devices (CM_Get_Device_ID_ListW)")?;
    Ok(ids
        .iter()
        .filter(|id| id.to_ascii_uppercase().contains("VEN_8086"))
        .filter_map(|id| locate(id))
        .find(|&node| {
            property_byte(node, &BASE_CLASS) == Some(0x06)
                && property_byte(node, &SUB_CLASS) == Some(0x01)
        })
        .and_then(|node| property_text(node, &DEVPKEY_Device_DeviceDesc)))
}

/// The instance ids of the PCI devices present now.
fn present_pci_devices() -> Option<Vec<String>> {
    let filter = wide("PCI");
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    // A device that arrives between the two calls makes the list outgrow the buffer, so then ask again.
    for _ in 0..3 {
        let mut length = 0u32;
        // SAFETY: `filter` is NUL-terminated and outlives the call; `length` is a valid place for the answer.
        if unsafe { CM_Get_Device_ID_List_SizeW(&mut length, filter.as_ptr(), flags) } != CR_SUCCESS
        {
            return None;
        }
        let mut list = vec![0u16; length as usize];
        // SAFETY: as above, and `list` is writable for the `length` UTF-16 units passed with it.
        match unsafe { CM_Get_Device_ID_ListW(filter.as_ptr(), list.as_mut_ptr(), length, flags) } {
            CR_SUCCESS => return Some(parse::multi_sz(&list)),
            CR_BUFFER_SMALL => continue,
            _ => return None,
        }
    }
    None
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
