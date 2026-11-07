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
    use std::sync::OnceLock;

    type MachPort = u32;
    const KERN_SUCCESS: i32 = 0;
    const HOST_CPU_LOAD_INFO: i32 = 3;
    const CPU_STATE_IDLE: usize = 2;

    extern "C" {
        fn mach_host_self() -> MachPort;
        fn host_statistics(host: MachPort, flavor: i32, info: *mut i32, count: *mut u32) -> i32;

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

    pub(super) fn memory() -> Option<(u64, u64)> {
        meeting_notes::macos_memory::used_and_total()
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
