//! Process cost, read from the kernel: CPU, peak RSS, and disk bytes.
//!
//! The harness measures with this sampler on every build, including the build
//! where every layer is off, so the sampler is compiled always and the `rusage`
//! feature adds only the layer that publishes a sample as a record.

#[cfg(feature = "rusage")]
use tracing::Subscriber;
#[cfg(feature = "rusage")]
use tracing_subscriber::layer::{Context, Layer};
#[cfg(feature = "rusage")]
use tracing_subscriber::registry::LookupSpan;

/// One reading of this process.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub cpu_user_secs: f64,
    pub cpu_system_secs: f64,
    pub peak_rss_bytes: Option<u64>,
    pub resident_rss_bytes: Option<u64>,
    pub live_alloc_bytes: Option<u64>,
    pub disk_read_bytes: Option<u64>,
    pub disk_write_bytes: Option<u64>,
}

/// Peak RSS and CPU of this process from `RUSAGE_SELF`. macOS reports resident
/// size in bytes, Linux in kibibytes.
pub fn cpu_and_peak_rss() -> (f64, f64, Option<u64>) {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if rc != 0 {
        return (0.0, 0.0, None);
    }
    let user = usage.ru_utime.tv_sec as f64 + usage.ru_utime.tv_usec as f64 / 1_000_000.0;
    let system = usage.ru_stime.tv_sec as f64 + usage.ru_stime.tv_usec as f64 / 1_000_000.0;
    #[cfg(target_os = "macos")]
    let peak = usage.ru_maxrss;
    #[cfg(not(target_os = "macos"))]
    let peak = usage.ru_maxrss * 1024;
    (user, system, Some(peak as u64))
}

/// Current resident memory, distinct from the process high-water mark.
#[allow(deprecated)]
pub fn resident_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::mach_task_basic_info_data_t = unsafe { std::mem::zeroed() };
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT as libc::mach_msg_type_number_t;
        let rc = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut info as *mut _ as *mut libc::integer_t,
                &mut count,
            )
        };
        (rc == libc::KERN_SUCCESS).then_some(info.resident_size as u64)
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages = text.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        (size > 0).then_some(pages.saturating_mul(size as u64))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn live_alloc_bytes() -> Option<u64> {
    crate::allocation::active().then(|| crate::allocation::live_bytes() as u64)
}

/// Cumulative disk I/O of this process as `(bytes read, bytes written)`.
pub fn disk_io_bytes() -> Option<(u64, u64)> {
    #[cfg(target_os = "macos")]
    let value = {
        let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::proc_pid_rusage(
                std::process::id() as libc::c_int,
                libc::RUSAGE_INFO_V2,
                &mut info as *mut libc::rusage_info_v2 as *mut libc::rusage_info_t,
            )
        };
        if rc != 0 {
            None
        } else {
            Some((info.ri_diskio_bytesread, info.ri_diskio_byteswritten))
        }
    };
    #[cfg(target_os = "linux")]
    let value = {
        let text = std::fs::read_to_string("/proc/self/io").ok()?;
        let field = |name: &str| -> Option<u64> {
            text.lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|value| value.trim().parse().ok())
        };
        Some((field("read_bytes:")?, field("write_bytes:")?))
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let value = None;
    value
}

pub fn sample() -> Usage {
    let (cpu_user_secs, cpu_system_secs, peak_rss_bytes) = cpu_and_peak_rss();
    let resident_rss_bytes = resident_rss_bytes();
    let live_alloc_bytes = live_alloc_bytes();
    let (disk_read_bytes, disk_write_bytes) = match disk_io_bytes() {
        Some((read, write)) => (Some(read), Some(write)),
        None => (None, None),
    };
    Usage {
        cpu_user_secs,
        cpu_system_secs,
        peak_rss_bytes,
        resident_rss_bytes,
        live_alloc_bytes,
        disk_read_bytes,
        disk_write_bytes,
    }
}

/// Publishes one usage sample as a record each time a span closes.
pub struct RusageLayer {
    #[cfg(feature = "rusage")]
    selected: std::collections::HashSet<String>,
}

#[cfg(feature = "rusage")]
#[derive(Default)]
struct ThreadCpu {
    entered_ns: Option<u64>,
    elapsed_ns: u64,
}

#[cfg(feature = "rusage")]
fn thread_cpu_ns() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut time: libc::timespec = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        (rc == 0).then(|| (time.tv_sec as u64) * 1_000_000_000 + time.tv_nsec as u64)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The usage layer when the feature is on and `HAFLEY_RUSAGE_SPANS` lists span
/// names to sample. The sampler itself is compiled on every build.
#[cfg(feature = "rusage")]
pub fn layer<S>() -> Option<Box<dyn Layer<S> + Send + Sync>>
where
    S: Subscriber + for<'a> LookupSpan<'a> + Send + Sync,
{
    let selected = std::env::var("HAFLEY_RUSAGE_SPANS")
        .ok()?
        .split(',')
        .map(str::to_string)
        .collect();
    Some(Layer::boxed(RusageLayer { selected }))
}

#[cfg(not(feature = "rusage"))]
pub fn layer<S>() -> Option<Box<dyn tracing_subscriber::Layer<S> + Send + Sync>>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    None
}

#[cfg(feature = "rusage")]
impl<S> Layer<S> for RusageLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if !self.selected.contains(span.name()) {
            return;
        }
        let Some(now) = thread_cpu_ns() else { return };
        let mut extensions = span.extensions_mut();
        if extensions.get_mut::<ThreadCpu>().is_none() {
            extensions.insert(ThreadCpu::default());
        }
        extensions.get_mut::<ThreadCpu>().unwrap().entered_ns = Some(now);
    }

    fn on_exit(&self, id: &tracing::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if !self.selected.contains(span.name()) {
            return;
        }
        let Some(now) = thread_cpu_ns() else { return };
        let mut extensions = span.extensions_mut();
        if let Some(cpu) = extensions.get_mut::<ThreadCpu>() {
            if let Some(start) = cpu.entered_ns.take() {
                cpu.elapsed_ns = cpu.elapsed_ns.saturating_add(now.saturating_sub(start));
            }
        }
    }

    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        if *span.metadata().level() > tracing::Level::INFO {
            return;
        }
        if !self.selected.contains(span.name()) {
            return;
        }
        let thread_cpu_ns = span
            .extensions()
            .get::<ThreadCpu>()
            .map(|cpu| cpu.elapsed_ns);
        let span_alloc_bytes = crate::allocation::allocated_bytes_by_id(id.into_u64());
        let usage = sample();
        tracing::debug!(
            target: crate::RUSAGE_TARGET,
            span = span.name(),
            "cpu.user_secs" = usage.cpu_user_secs,
            "cpu.system_secs" = usage.cpu_system_secs,
            "cpu.thread_ns" = thread_cpu_ns.unwrap_or_default(),
            "mem.rss_bytes" = usage.peak_rss_bytes.unwrap_or_default(),
            "mem.rss_end_bytes" = usage.resident_rss_bytes.unwrap_or_default(),
            "mem.alloc_end_bytes" = usage.live_alloc_bytes.unwrap_or_default(),
            "mem.span_alloc_bytes" = span_alloc_bytes.unwrap_or_default(),
            "io.read_bytes" = usage.disk_read_bytes.unwrap_or_default(),
            "io.write_bytes" = usage.disk_write_bytes.unwrap_or_default(),
            "process usage sampled"
        );
    }
}

#[cfg(test)]
mod tests {
    use oh::test;

    #[test]
    fn current_rss_is_a_resident_sample() {
        let usage = super::sample();
        let resident = usage.resident_rss_bytes.expect("current RSS is available");
        let peak = usage.peak_rss_bytes.expect("peak RSS is available");
        assert!(resident > 0);
        assert!(peak >= resident);
    }
}
