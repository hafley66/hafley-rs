//! A watchdog thread exits the process once its footprint passes the cap. macOS resident size
//! omits compressed pages: a run read as 3 GB resident was 79 GB when the kernel killed it.

use std::time::Duration;

/// Gibibytes; `0` turns the watchdog off.
pub const CAP_VARIABLE: &str = "HAFLEY_MEMCAP_GB";
pub const DEFAULT_CAP_GB: u64 = 8;
/// Exit code of a capped process, the shell's code for SIGKILL.
pub const EXIT_CODE: i32 = 137;
const POLL: Duration = Duration::from_millis(200);

/// This process's physical footprint in bytes, compressed and swapped pages included.
#[cfg(target_os = "macos")]
pub fn footprint() -> Option<u64> {
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut info as *mut libc::rusage_info_v2 as *mut libc::rusage_info_t,
        )
    };
    (rc == 0).then_some(info.ri_phys_footprint)
}

/// `VmRSS` plus `VmSwap` from `/proc/self/status`.
#[cfg(not(target_os = "macos"))]
pub fn footprint() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kib = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
    };
    Some((kib("VmRSS:")? + kib("VmSwap:").unwrap_or(0)) * 1024)
}

/// The cap in bytes from `HAFLEY_MEMCAP_GB`, `None` when it is `0`.
pub fn cap_bytes() -> Option<u64> {
    let gb = std::env::var(CAP_VARIABLE)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_CAP_GB);
    (gb > 0).then_some(gb << 30)
}

/// Starts the watchdog for this process. Past the cap it writes one line naming `label`, the
/// footprint and the cap to stderr, then exits with `EXIT_CODE`.
pub fn spawn(label: &'static str) -> Option<std::thread::JoinHandle<()>> {
    let cap = cap_bytes()?;
    footprint()?;
    std::thread::Builder::new()
        .name("memcap".into())
        .spawn(move || loop {
            if let Some(used) = footprint().filter(|used| *used > cap) {
                let gib = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
                eprintln!(
                    "memcap: {label} footprint {:.2} GiB over cap {:.2} GiB ({CAP_VARIABLE}); exiting {EXIT_CODE}",
                    gib(used),
                    gib(cap)
                );
                tracing::error!(label, used, cap, "memcap exceeded");
                std::process::exit(EXIT_CODE);
            }
            std::thread::sleep(POLL);
        })
        .ok()
}
