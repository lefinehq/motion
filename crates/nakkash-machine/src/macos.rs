//! macOS: the kernel's variables, read by name, and the process's own
//! resource record.

use std::ffi::CStr;
use std::mem::MaybeUninit;

use crate::parse;
use crate::{Pressure, Swap};

/// The bytes of the kernel variable `name`, which must be `N` bytes long;
/// `None` when it does not exist or has another size.
fn sysctl<const N: usize>(name: &CStr) -> Option<[u8; N]> {
    let mut value = [0u8; N];
    let mut size = N;
    // SAFETY: `name` is a NUL-terminated string; `value` is a buffer of
    // `size` bytes, which the kernel writes at most `size` bytes into and
    // sets `size` to what it wrote; no new value is given (null, 0).
    let done = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (done == 0 && size == N).then_some(value)
}

/// The 64-bit kernel variable `name`.
fn sysctl_u64(name: &CStr) -> Option<u64> {
    sysctl::<8>(name).map(u64::from_ne_bytes)
}

/// The 32-bit kernel variable `name`.
fn sysctl_i32(name: &CStr) -> Option<i32> {
    sysctl::<4>(name).map(i32::from_ne_bytes)
}

pub(crate) fn memory() -> Option<u64> {
    sysctl_u64(c"hw.memsize")
}

/// The performance cores, on processors that have two kinds.
pub(crate) fn performance_cores() -> Option<usize> {
    let cores = sysctl_i32(c"hw.perflevel0.physicalcpu")?;
    usize::try_from(cores).ok().filter(|&n| n > 0)
}

pub(crate) fn pressure() -> Pressure {
    sysctl_i32(c"kern.memorystatus_vm_pressure_level").map_or(Pressure::Normal, parse::kernel_level)
}

/// The process's resource record, from the kernel.
fn record() -> Option<libc::rusage_info_v2> {
    let mut info = MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let pid = i32::try_from(std::process::id()).ok()?;
    // SAFETY: for `RUSAGE_INFO_V2` the kernel writes one `rusage_info_v2`
    // into the buffer, which is one (the binding types it as a pointer to
    // `rusage_info_t`, which is what the C function takes).
    let done = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    if done != 0 {
        return None;
    }
    // SAFETY: a zeroed `rusage_info_v2` is valid, as it holds only integers
    // and bytes, and the call above filled it.
    Some(unsafe { info.assume_init() })
}

/// The bytes the process wrote to storage, from its resource record.
pub(crate) fn written() -> Option<u64> {
    record().map(|info| info.ri_diskio_byteswritten)
}

/// The memory the process holds resident now.
pub(crate) fn resident() -> Option<u64> {
    record().map(|info| info.ri_resident_size)
}

pub(crate) fn swap() -> Option<Swap> {
    // `struct xsw_usage`: total, available and used, each 64 bits, then
    // the page size and whether swap is encrypted.
    let usage = sysctl::<{ std::mem::size_of::<libc::xsw_usage>() }>(c"vm.swapusage")?;
    let used: [u8; 8] = usage[16..24].try_into().ok()?;
    Some(Swap::Used(u64::from_ne_bytes(used)))
}
