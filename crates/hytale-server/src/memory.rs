//! Process memory snapshots for benchmarks and soak tests.
//!
//! Linux exposes the resident set and high-water mark through procfs. Other
//! platforms return `None` so callers can keep a portable report.

/// Current resident set size in bytes, when the platform exposes it.
pub fn resident_bytes() -> Option<u64> {
    read_proc_status_bytes("VmRSS:")
}

/// Peak resident set size in bytes, when the platform exposes it.
pub fn peak_resident_bytes() -> Option<u64> {
    read_proc_status_bytes("VmHWM:")
}

#[cfg(target_os = "linux")]
fn read_proc_status_bytes(prefix: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with(prefix))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib.saturating_mul(1024))
}

#[cfg(not(target_os = "linux"))]
fn read_proc_status_bytes(_prefix: &str) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_reports_its_own_memory_on_supported_platforms() {
        if cfg!(target_os = "linux") {
            let resident = resident_bytes().expect("resident set");
            let peak = peak_resident_bytes().expect("peak resident set");
            assert!(resident > 0);
            assert!(peak >= resident);
        } else {
            assert_eq!(resident_bytes(), None);
            assert_eq!(peak_resident_bytes(), None);
        }
    }
}
