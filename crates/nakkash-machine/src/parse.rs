//! The text files Linux reports memory and I/O in, read without the
//! system: kept apart from the reading so every platform tests them.

use crate::Pressure;

/// The value of the line `key:` in a `/proc/meminfo`-style file, in bytes
/// (its figure is in KiB).
pub(crate) fn meminfo_bytes(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
        Some(kib * 1024)
    })
}

/// The value of the line `key value` in a `/proc/self/io` or
/// `/proc/vmstat` file, `key:` or `key` alike.
pub(crate) fn keyed(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let name = words.next()?.trim_end_matches(':');
        (name == key).then(|| words.next()?.parse().ok())?
    })
}

/// A cgroup's `memory.max`: a number of bytes, or `None` for `max`, no
/// limit.
pub(crate) fn limit(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// The cgroup v2 path of the process, from a `/proc/self/cgroup` file: the
/// line `0::/path`.
pub(crate) fn cgroup_path(text: &str) -> Option<&str> {
    text.lines().find_map(|line| line.strip_prefix("0::"))
}

/// Pressure stall information: the share of the last ten seconds in which
/// some tasks, and all tasks, waited for memory, in percent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Psi {
    /// `some avg10`.
    pub some: f64,
    /// `full avg10`.
    pub full: f64,
}

impl Psi {
    /// The stall information in a `memory.pressure` file.
    pub fn parse(text: &str) -> Option<Psi> {
        let avg10 = |kind: &str| -> Option<f64> {
            let line = text.lines().find(|l| l.starts_with(kind))?;
            let field = line
                .split_whitespace()
                .find_map(|w| w.strip_prefix("avg10="))?;
            field.parse().ok()
        };
        Some(Psi {
            some: avg10("some")?,
            full: avg10("full").unwrap_or(0.0),
        })
    }

    /// The pressure it reads as: critical once every task waited 5 % of the
    /// time, a warning once some did 10 %.
    pub fn pressure(self) -> Pressure {
        if self.full >= 5.0 {
            Pressure::Critical
        } else if self.some >= 10.0 {
            Pressure::Warning
        } else {
            Pressure::Normal
        }
    }
}

/// The pressure of using `current` bytes of a limit of `max`: critical
/// from 95 %, a warning from 85 %.
pub(crate) fn used_pressure(current: u64, max: u64) -> Pressure {
    let share = current as f64 / max.max(1) as f64;
    if share >= 0.95 {
        Pressure::Critical
    } else if share >= 0.85 {
        Pressure::Warning
    } else {
        Pressure::Normal
    }
}

/// The pressure of having `available` bytes of `total` left: critical
/// under 5 %, a warning under 10 %.
pub(crate) fn available_pressure(available: u64, total: u64) -> Pressure {
    let share = available as f64 / total.max(1) as f64;
    if share < 0.05 {
        Pressure::Critical
    } else if share < 0.10 {
        Pressure::Warning
    } else {
        Pressure::Normal
    }
}

/// macOS's `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning,
/// 4 critical.
pub(crate) fn kernel_level(level: i32) -> Pressure {
    match level {
        4.. => Pressure::Critical,
        2 | 3 => Pressure::Warning,
        _ => Pressure::Normal,
    }
}
