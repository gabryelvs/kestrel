//! Windows performance counters (PDH), the numbers Task Manager shows. They are system-wide, so Kestrel does not
//! open a handle to any app to read them. Read-only: a query only ever reads counters.
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_INVALID_ARGUMENT, PDH_MORE_DATA,
};

/// Format flag: don't cap percentages at 100, so a program busy on four cores reads 400.
/// (windows-sys doesn't export it; PDH_FMT_NOCAP100 is 0x8000 in pdh.h.)
pub const FMT_NOCAP100: u32 = 0x0000_8000;

// The item buffer is a Vec<u64>, so it must be aligned at least as strictly as one item.
const _: () = assert!(align_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() <= align_of::<u64>());

/// A live query for one counter. Closed (with the counter) when dropped.
pub struct Query {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// SAFETY: PDH handles can be used from any thread, they are not tied to the one that opened them. A `Query` has a
// single owner and is never used from two threads at once: the live load sampler and the clock sampler, which hold
// them, each sit behind a Mutex in the app's state.
unsafe impl Send for Query {}

impl Query {
    /// Opens a query for the counter at `counter_path`, written in English whatever language Windows speaks,
    /// for example `\Process(*)\% Processor Time`. None if Windows can't provide that counter.
    pub fn open(counter_path: &str) -> Option<Self> {
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        // SAFETY: a null data source means live data; `query` is a valid place for the new handle.
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
            return None;
        }
        // From here on, dropping `opened` closes the query, however this function ends.
        let mut opened = Self {
            query,
            counter: std::ptr::null_mut(),
        };
        let path: Vec<u16> = counter_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `path` is NUL-terminated and outlives the call, `opened.query` is open, and
        // `opened.counter` is a valid place for the new handle.
        let status =
            unsafe { PdhAddEnglishCounterW(opened.query, path.as_ptr(), 0, &mut opened.counter) };
        (status == 0).then_some(opened)
    }

    /// Takes a sample. A rate (a percentage, say) is the change between two samples, so collect twice, a while
    /// apart, before reading it.
    pub fn collect(&self) -> bool {
        // SAFETY: the query stays open as long as `self` exists.
        unsafe { PdhCollectQueryData(self.query) == 0 }
    }

    /// One `(instance name, value)` per instance of a wildcard counter that has valid data, from the last two
    /// samples. Values are doubles whatever `flags` says; `flags` adds modifiers such as `FMT_NOCAP100`.
    pub fn formatted_array(&self, flags: u32) -> Option<Vec<(String, f64)>> {
        let format = flags | PDH_FMT_DOUBLE;
        let (mut bytes, mut count) = (0u32, 0u32);
        // A first call with no buffer says how much room the items need. A process can start before the second
        // call, which then finds the buffer too small. PDH's new size can't be trusted then (and some Windows
        // releases answer INVALID_ARGUMENT instead of MORE_DATA), so start over with a fresh first call.
        let mut buffer: Vec<u64> = Vec::new();
        for _ in 0..6 {
            let sizing = buffer.is_empty();
            let items = if sizing {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()
            };
            // SAFETY: the counter is open as long as `self` exists. `items` is null with `bytes` 0, or points to
            // `buffer`, which is 8-byte aligned and exactly `bytes` long.
            let status = unsafe {
                PdhGetFormattedCounterArrayW(self.counter, format, &mut bytes, &mut count, items)
            };
            match status {
                0 => return Some(read_items(&buffer, count as usize)),
                PDH_MORE_DATA if sizing => {
                    buffer = vec![0u64; bytes.div_ceil(8) as usize];
                    bytes = u32::try_from(size_of_val(buffer.as_slice())).ok()?;
                }
                PDH_MORE_DATA | PDH_INVALID_ARGUMENT if !sizing => {
                    buffer.clear();
                    bytes = 0;
                }
                _ => return None,
            }
        }
        None
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: the query was opened in `open` and is closed only here. Closing it frees its counter too.
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// Reads the `count` items PDH wrote at the start of `buffer`; the names they point to are stored after them.
/// Items without valid data are skipped, and so is anything that doesn't lie inside the buffer.
fn read_items(buffer: &[u64], count: usize) -> Vec<(String, f64)> {
    let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let len = size_of_val(buffer);
    if count
        .checked_mul(item_size)
        .is_none_or(|needed| needed > len)
    {
        return Vec::new();
    }
    let base = buffer.as_ptr().cast::<u8>();
    (0..count)
        .filter_map(|i| {
            // SAFETY: item `i` lies inside the buffer (checked above), which is 8-byte aligned and initialised.
            let item = unsafe {
                base.add(i * item_size)
                    .cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()
                    .read()
            };
            let status = item.FmtValue.CStatus;
            if status != PDH_CSTATUS_VALID_DATA && status != PDH_CSTATUS_NEW_DATA {
                return None;
            }
            // The name is a pointer into this same buffer; only trust it if it really lies inside.
            let offset = (item.szName as usize).checked_sub(base as usize)?;
            if offset >= len || offset % 2 != 0 {
                return None;
            }
            // SAFETY: `offset` is even and inside the buffer, so the u16s from there to the buffer's end are
            // in bounds, aligned and initialised.
            let units = unsafe {
                std::slice::from_raw_parts(base.add(offset).cast::<u16>(), (len - offset) / 2)
            };
            let name = String::from_utf16_lossy(&units[..units.iter().position(|&c| c == 0)?]);
            // SAFETY: the format always includes PDH_FMT_DOUBLE, so the value is the `doubleValue` member.
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };
            Some((name, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Performance::{
        PDH_CALC_NEGATIVE_DENOMINATOR, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_0,
    };

    const VALID: u32 = PDH_CSTATUS_VALID_DATA;

    /// A buffer laid out the way PDH fills it: the items first, then their names, all in one buffer.
    fn buffer_of(entries: &[(&str, u32, f64)]) -> Vec<u64> {
        let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let names: Vec<Vec<u16>> = entries
            .iter()
            .map(|(name, ..)| name.encode_utf16().chain(std::iter::once(0)).collect())
            .collect();
        let name_bytes: usize = names.iter().map(|name| name.len() * 2).sum();
        let mut buffer = vec![0u64; (item_size * entries.len() + name_bytes).div_ceil(8)];
        let base = buffer.as_mut_ptr().cast::<u8>();
        let mut name_at = item_size * entries.len();
        for (i, ((_, status, value), name)) in entries.iter().zip(&names).enumerate() {
            // SAFETY: the buffer was sized for every item and every name.
            unsafe {
                let name_ptr = base.add(name_at).cast::<u16>();
                std::ptr::copy_nonoverlapping(name.as_ptr(), name_ptr, name.len());
                let item = PDH_FMT_COUNTERVALUE_ITEM_W {
                    szName: name_ptr,
                    FmtValue: PDH_FMT_COUNTERVALUE {
                        CStatus: *status,
                        Anonymous: PDH_FMT_COUNTERVALUE_0 {
                            doubleValue: *value,
                        },
                    },
                };
                base.add(i * item_size)
                    .cast::<PDH_FMT_COUNTERVALUE_ITEM_W>()
                    .write(item);
            }
            name_at += name.len() * 2;
        }
        buffer
    }

    #[test]
    fn reads_names_and_values() {
        let buffer = buffer_of(&[
            ("chrome", VALID, 12.5),
            ("chrome#1", PDH_CSTATUS_NEW_DATA, 800.0),
            ("Memory Compression", VALID, 0.0),
        ]);
        assert_eq!(
            read_items(&buffer, 3),
            vec![
                ("chrome".to_string(), 12.5),
                ("chrome#1".to_string(), 800.0),
                ("Memory Compression".to_string(), 0.0),
            ]
        );
    }

    #[test]
    fn skips_items_without_valid_data() {
        let buffer = buffer_of(&[
            ("new", PDH_CALC_NEGATIVE_DENOMINATOR, 99.0),
            ("old", VALID, 1.0),
        ]);
        assert_eq!(read_items(&buffer, 2), vec![("old".to_string(), 1.0)]);
    }

    #[test]
    fn an_empty_answer_is_an_empty_list() {
        assert!(read_items(&[], 0).is_empty());
    }

    #[test]
    fn a_count_bigger_than_the_buffer_reads_nothing() {
        let buffer = buffer_of(&[("a", VALID, 1.0)]);
        assert!(read_items(&buffer, 1_000_000).is_empty());
        assert!(read_items(&buffer, usize::MAX).is_empty());
    }

    #[test]
    fn a_name_pointing_outside_the_buffer_is_skipped() {
        let mut buffer = buffer_of(&[("inside", VALID, 1.0), ("outside", VALID, 2.0)]);
        let elsewhere = [0x0041u16, 0];
        let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        // SAFETY: item 1 lies inside the buffer; only its name pointer is changed.
        unsafe {
            let second = buffer
                .as_mut_ptr()
                .cast::<u8>()
                .add(item_size)
                .cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            (*second).szName = elsewhere.as_ptr().cast_mut();
        }
        assert_eq!(read_items(&buffer, 2), vec![("inside".to_string(), 1.0)]);
    }

    #[test]
    fn a_name_without_an_end_is_skipped() {
        let mut buffer = buffer_of(&[("cut", VALID, 1.0)]);
        // Overwrite everything after the item (the name and the padding) with letters: there is no NUL left.
        let items = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>().div_ceil(8);
        buffer[items..].fill(0x0041_0041_0041_0041);
        assert!(read_items(&buffer, 1).is_empty());
    }
}
