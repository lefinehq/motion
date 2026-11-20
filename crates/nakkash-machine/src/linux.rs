//! Linux: `/proc` and the process's control group (cgroup v2, or v1's
//! memory controller), read as files; nothing here is unsafe.

use std::fs;
use std::path::PathBuf;

use crate::parse::{self, Psi};
use crate::{Pressure, Swap};

/// The folder of the process's cgroup v2, where its `memory.*` files are:
/// its path under `/sys/fs/cgroup`, or that folder itself inside a
/// container that sees only its own.
fn cgroup() -> Option<PathBuf> {
    let root = PathBuf::from("/sys/fs/cgroup");
    let own = fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|text| parse::cgroup_path(&text).map(|p| root.join(p.trim_start_matches('/'))));
    [own, Some(root)]
        .into_iter()
        .flatten()
        .find(|dir| dir.join("memory.max").exists())
}

/// The cgroup's memory limit, from v2's `memory.max` or v1's
/// `memory.limit_in_bytes`; `None` without one.
fn cgroup_limit() -> Option<u64> {
    if let Some(dir) = cgroup() {
        return parse::limit(&fs::read_to_string(dir.join("memory.max")).ok()?);
    }
    let v1 = fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes").ok()?;
    // v1 writes no limit as a number near the top of the 64-bit range.
    parse::limit(&v1).filter(|&bytes| bytes < 1 << 60)
}

pub(crate) fn memory() -> Option<u64> {
    let total = parse::meminfo_bytes(&fs::read_to_string("/proc/meminfo").ok()?, "MemTotal")?;
    Some(cgroup_limit().map_or(total, |limit| limit.min(total)))
}

pub(crate) fn pressure() -> Pressure {
    let mut level = Pressure::Normal;
    let dir = cgroup();
    if let Some(dir) = &dir {
        let read = |name: &str| fs::read_to_string(dir.join(name)).ok();
        if let (Some(current), Some(max)) = (
            read("memory.current").and_then(|t| t.trim().parse().ok()),
            read("memory.max").and_then(|t| parse::limit(&t)),
        ) {
            level = level.max(parse::used_pressure(current, max));
        }
    }
    let psi = dir
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("memory.pressure")).ok())
        .or_else(|| fs::read_to_string("/proc/pressure/memory").ok())
        .and_then(|text| Psi::parse(&text));
    if let Some(psi) = psi {
        level = level.max(psi.pressure());
    }
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo")
        && let (Some(available), Some(total)) = (
            parse::meminfo_bytes(&meminfo, "MemAvailable"),
            parse::meminfo_bytes(&meminfo, "MemTotal"),
        )
    {
        level = level.max(parse::available_pressure(available, total));
    }
    level
}

/// The bytes the process caused to be written to storage, from
/// `/proc/self/io`'s `write_bytes`.
pub(crate) fn written() -> Option<u64> {
    parse::keyed(&fs::read_to_string("/proc/self/io").ok()?, "write_bytes")
}

/// The memory the process holds resident now: `/proc/self/statm`'s second
/// figure, in pages.
pub(crate) fn resident() -> Option<u64> {
    let statm = fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * crate::unix::page_size())
}

pub(crate) fn swap() -> Option<Swap> {
    let vmstat = fs::read_to_string("/proc/vmstat").ok()?;
    Some(Swap::Pages {
        ins: parse::keyed(&vmstat, "pswpin")?,
        outs: parse::keyed(&vmstat, "pswpout")?,
    })
}
