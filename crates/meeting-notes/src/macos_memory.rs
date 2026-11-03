//! Physical memory accounting shared by load admission and the desktop monitor.
use std::{ffi::c_void, sync::OnceLock};

type MachPort = u32;
const KERN_SUCCESS: i32 = 0;
const HOST_VM_INFO64: i32 = 4;
/// `vm_statistics64` from `<mach/vm_statistics.h>`.
#[repr(C)]
#[derive(Default)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

extern "C" {
    fn mach_host_self() -> MachPort;
    fn host_statistics64(host: MachPort, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    fn host_page_size(host: MachPort, size: *mut usize) -> i32;
    fn sysctlbyname(
        name: *const std::ffi::c_char,
        old: *mut c_void,
        old_length: *mut usize,
        new: *mut c_void,
        new_length: usize,
    ) -> i32;
}

/// One host port for the process; asking again would add a port reference each time.
fn host() -> MachPort {
    static HOST: OnceLock<MachPort> = OnceLock::new();
    // SAFETY: returns a send right to the host port and has no preconditions.
    *HOST.get_or_init(|| unsafe { mach_host_self() })
}

/// Used memory as Activity Monitor counts it: app memory, wired and compressed pages.
pub fn used_and_total() -> Option<(u64, u64)> {
    let mut total = 0u64;
    let mut length = std::mem::size_of::<u64>();
    // SAFETY: a NUL-terminated name and a u64 buffer with its size.
    let result = unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut total as *mut u64).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length != std::mem::size_of::<u64>() || total == 0 {
        return None;
    }
    let mut page = 0usize;
    // SAFETY: a valid out-parameter for the page size.
    if unsafe { host_page_size(host(), &mut page) } != KERN_SUCCESS || page == 0 {
        return None;
    }
    let mut stats = VmStatistics64::default();
    let mut count =
        u32::try_from(std::mem::size_of::<VmStatistics64>() / std::mem::size_of::<i32>()).ok()?;
    // SAFETY: a vm_statistics64 buffer and its length in integers.
    let result = unsafe {
        host_statistics64(
            host(),
            HOST_VM_INFO64,
            (&mut stats as *mut VmStatistics64).cast(),
            &mut count,
        )
    };
    if result != KERN_SUCCESS {
        return None;
    }
    let pages = u64::from(stats.internal_page_count)
        .saturating_sub(u64::from(stats.purgeable_count))
        .checked_add(u64::from(stats.wire_count))?
        .checked_add(u64::from(stats.compressor_page_count))?;
    let used = pages.checked_mul(u64::try_from(page).ok()?)?.min(total);
    Some((used, total))
}
