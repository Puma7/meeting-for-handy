//! System-wide processor and memory load for the recording status, read without extra
//! libraries. A value that cannot be read is `None`; nothing is estimated.
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemLoad {
    /// Share of processor time in use since the previous reading, 0–100.
    pub cpu_percent: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

/// Busy and total processor time; only the difference of two readings is meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuTicks {
    busy: u64,
    total: u64,
}

/// A reading older than this no longer describes the current load, for example after the
/// window was hidden or between two recordings.
const MAX_SAMPLE_AGE: Duration = Duration::from_secs(10);

static PREVIOUS: Mutex<Option<(Instant, CpuTicks)>> = Mutex::new(None);

/// Reads the current load. The processor share covers the time since the previous call,
/// so the first call after start, or after a pause longer than `MAX_SAMPLE_AGE`, reports
/// no processor value.
pub(crate) fn sample() -> SystemLoad {
    let cpu_percent = platform::cpu_ticks().and_then(|now| {
        let at = Instant::now();
        let before = PREVIOUS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace((at, now));
        recent_share(before, at, now)
    });
    let memory = platform::memory();
    SystemLoad {
        cpu_percent,
        memory_used_bytes: memory.map(|(used, _)| used),
        memory_total_bytes: memory.map(|(_, total)| total),
    }
}

/// The share since a previous reading that is recent enough to describe the current load.
fn recent_share(before: Option<(Instant, CpuTicks)>, at: Instant, now: CpuTicks) -> Option<f64> {
    let (then, before) = before?;
    if at.saturating_duration_since(then) > MAX_SAMPLE_AGE {
        return None;
    }
    cpu_share(before, now)
}

/// A counter that went backwards (wrapped or reset) gives no value rather than a wrong one.
fn cpu_share(before: CpuTicks, now: CpuTicks) -> Option<f64> {
    let total = now.total.checked_sub(before.total)?;
    let busy = now.busy.checked_sub(before.busy)?;
    if total == 0 || busy > total {
        return None;
    }
    Some(busy as f64 * 100.0 / total as f64)
}

/// The aggregate `cpu` line of `/proc/stat`: idle and I/O wait count as not busy.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_proc_stat(stat: &str) -> Option<CpuTicks> {
    let line = stat.lines().find(|line| line.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if fields.len() < 4 {
        return None;
    }
    let total = fields
        .iter()
        .try_fold(0u64, |sum, value| sum.checked_add(*value))?;
    let idle = fields[3].checked_add(fields.get(4).copied().unwrap_or(0))?;
    Some(CpuTicks {
        busy: total.checked_sub(idle)?,
        total,
    })
}

/// Used and total memory in bytes from `/proc/meminfo`; used excludes reclaimable caches.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(info: &str) -> Option<(u64, u64)> {
    let kib = |key: &str| -> Option<u64> {
        let line = info.lines().find(|line| line.starts_with(key))?;
        line[key.len()..].split_whitespace().next()?.parse().ok()
    };
    let total = kib("MemTotal:")?.checked_mul(1024)?;
    let available = kib("MemAvailable:")?.checked_mul(1024)?;
    Some((total.checked_sub(available)?, total))
}

#[cfg(target_os = "linux")]
mod platform {
    use super::CpuTicks;

    pub(super) fn cpu_ticks() -> Option<CpuTicks> {
        super::parse_proc_stat(&std::fs::read_to_string("/proc/stat").ok()?)
    }

    pub(super) fn memory() -> Option<(u64, u64)> {
        super::parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }
}

#[cfg(windows)]
mod platform {
    use super::CpuTicks;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    fn ticks(time: &FileTime) -> u64 {
        (u64::from(time.high) << 32) | u64::from(time.low)
    }

    pub(super) fn cpu_ticks() -> Option<CpuTicks> {
        let (mut idle, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        // SAFETY: three valid, exclusively borrowed FILETIME out-parameters.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        // Kernel time includes the idle time.
        let total = ticks(&kernel).checked_add(ticks(&user))?;
        Some(CpuTicks {
            busy: total.checked_sub(ticks(&idle))?,
            total,
        })
    }

    pub(super) fn memory() -> Option<(u64, u64)> {
        let mut status = MemoryStatusEx {
            length: u32::try_from(std::mem::size_of::<MemoryStatusEx>()).ok()?,
            ..Default::default()
        };
        // SAFETY: a valid MEMORYSTATUSEX whose length field names its size.
        if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
            return None;
        }
        Some((
            status.total_phys.checked_sub(status.avail_phys)?,
            status.total_phys,
        ))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::CpuTicks;
    use std::{ffi::c_void, sync::OnceLock};

    type MachPort = u32;
    const KERN_SUCCESS: i32 = 0;
    const HOST_CPU_LOAD_INFO: i32 = 3;
    const HOST_VM_INFO64: i32 = 4;
    const CPU_STATE_IDLE: usize = 2;

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
        fn host_statistics(host: MachPort, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
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

    pub(super) fn cpu_ticks() -> Option<CpuTicks> {
        let mut ticks = [0u32; 4];
        let mut count = 4u32;
        // SAFETY: a buffer of HOST_CPU_LOAD_INFO_COUNT integers and its length.
        let result = unsafe {
            host_statistics(
                host(),
                HOST_CPU_LOAD_INFO,
                ticks.as_mut_ptr().cast(),
                &mut count,
            )
        };
        if result != KERN_SUCCESS || count != 4 {
            return None;
        }
        let total: u64 = ticks.iter().map(|&value| u64::from(value)).sum();
        Some(CpuTicks {
            busy: total - u64::from(ticks[CPU_STATE_IDLE]),
            total,
        })
    }

    /// Used memory as Activity Monitor counts it: app memory, wired and compressed pages.
    pub(super) fn memory() -> Option<(u64, u64)> {
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
            u32::try_from(std::mem::size_of::<VmStatistics64>() / std::mem::size_of::<i32>())
                .ok()?;
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
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod platform {
    use super::CpuTicks;

    pub(super) fn cpu_ticks() -> Option<CpuTicks> {
        None
    }

    pub(super) fn memory() -> Option<(u64, u64)> {
        None
    }
}

/// Admission only. A slow in-flight request is never cancelled by this reading.
pub(crate) fn workspace_memory_ready() -> bool {
    platform::memory()
        .is_none_or(|(used, total)| total.saturating_sub(used) >= 2 * 1024 * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_share_follows_the_difference_of_two_readings() {
        let before = CpuTicks {
            busy: 100,
            total: 400,
        };
        let now = CpuTicks {
            busy: 175,
            total: 500,
        };
        assert_eq!(cpu_share(before, now), Some(75.0));
        // No time passed, or a counter went backwards: no value instead of a wrong one.
        assert_eq!(cpu_share(now, now), None);
        assert_eq!(cpu_share(now, before), None);
    }

    #[test]
    fn a_reading_from_long_ago_gives_no_processor_value() {
        let before = CpuTicks {
            busy: 100,
            total: 400,
        };
        let now = CpuTicks {
            busy: 175,
            total: 500,
        };
        let then = Instant::now();
        assert_eq!(
            recent_share(Some((then, before)), then + Duration::from_secs(2), now),
            Some(75.0)
        );
        // After a hidden window or between recordings the average would span minutes.
        assert_eq!(
            recent_share(
                Some((then, before)),
                then + MAX_SAMPLE_AGE + Duration::from_millis(1),
                now
            ),
            None
        );
        assert_eq!(recent_share(None, then, now), None);
    }

    #[test]
    fn proc_stat_counts_idle_and_io_wait_as_not_busy() {
        let stat = "cpu  100 20 30 800 50 0 0 0 0 0\ncpu0 1 2 3 4 5 6 7 8 0 0\n";
        assert_eq!(
            parse_proc_stat(stat),
            Some(CpuTicks {
                busy: 150,
                total: 1000
            })
        );
        assert_eq!(parse_proc_stat("cpu0 1 2 3 4\n"), None);
        assert_eq!(parse_proc_stat("cpu  1 2 x 4\n"), None);
    }

    #[test]
    fn meminfo_reports_used_memory_without_reclaimable_caches() {
        let info = "MemTotal:       16000000 kB\nMemFree:  1000 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(
            parse_meminfo(info),
            Some((12_000_000 * 1024, 16_000_000 * 1024))
        );
        assert_eq!(parse_meminfo("MemTotal: 10 kB\n"), None);
        assert_eq!(
            parse_meminfo("MemTotal: 10 kB\nMemAvailable: 20 kB\n"),
            None
        );
    }

    #[test]
    fn a_reading_on_this_computer_is_plausible() {
        let _ = sample();
        let load = sample();
        if let (Some(used), Some(total)) = (load.memory_used_bytes, load.memory_total_bytes) {
            assert!(used <= total && total > 0);
        }
        if let Some(cpu) = load.cpu_percent {
            assert!((0.0..=100.0).contains(&cpu));
        }
    }
}
