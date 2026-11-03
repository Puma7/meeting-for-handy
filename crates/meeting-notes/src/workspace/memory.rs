//! Scalar admission checks only; never cancels an in-flight inference or reads meeting data.
use anyhow::{ensure, Result};
pub(crate) fn require_load_headroom(weights: u64) -> Result<()> {
    check(weights, available())
}
fn check(weights: u64, available: Option<u64>) -> Result<()> {
    ensure!(
        weights > 0,
        "Modellgröße fehlt. Ein bereits bewusst geladenes Modell verwenden."
    );
    // Initial conservative policy for an 8k context: weights plus 30% and 2 GiB reserve.
    // This is admission, not a promise about actual model allocation or device capture.
    let required = weights
        .checked_mul(13)
        .and_then(|n| n.checked_div(10))
        .and_then(|n| n.checked_add(2 * 1024 * 1024 * 1024))
        .ok_or_else(|| anyhow::anyhow!("Modellgröße ist ungültig."))?;
    ensure!(available.is_some_and(|n|n>=required),"Zu wenig bestätigter freier Arbeitsspeicher für eine eigene Modellladung. Ein kleineres bereits vorhandenes Modell wählen oder später erneut starten; Aufnahme und gespeicherte Daten bleiben erhalten.");
    Ok(())
}
#[cfg(windows)]
fn available() -> Option<u64> {
    #[repr(C)]
    struct Memory {
        length: u32,
        load: u32,
        total: u64,
        available: u64,
        page_total: u64,
        page_available: u64,
        virtual_total: u64,
        virtual_available: u64,
        extended: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(value: *mut Memory) -> i32;
    }
    let mut value = Memory {
        length: std::mem::size_of::<Memory>() as u32,
        load: 0,
        total: 0,
        available: 0,
        page_total: 0,
        page_available: 0,
        virtual_total: 0,
        virtual_available: 0,
        extended: 0,
    };
    // SAFETY: initialized repr(C) structure with its exact Windows API size.
    (unsafe { GlobalMemoryStatusEx(&mut value) } != 0).then_some(value.available)
}
#[cfg(target_os = "linux")]
fn available() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:").and_then(|s| {
                s.split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(1024)
            })
        })
}
#[cfg(not(any(windows, target_os = "linux")))]
fn available() -> Option<u64> {
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_load_requires_reserve_and_known_positive_size() {
        let size = 1000;
        let need = 1300 + 2 * 1024 * 1024 * 1024;
        assert!(check(size, Some(need)).is_ok());
        assert!(check(size, Some(need - 1)).is_err());
        assert!(check(size, None).is_err());
        assert!(check(0, Some(u64::MAX)).is_err());
        assert!(check(u64::MAX, Some(u64::MAX)).is_err());
    }
}
