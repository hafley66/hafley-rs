//! The device-sync seam shared by the durable stage store and commit engine.
//! Levels and their guarantees are quoted from macOS `fcntl(2)`.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tempfile::NamedTempFile;

/// How far a write has to reach before the next step may proceed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyncLevel {
    /// `fsync(2)`: pages reach the device, nothing ordered, nothing durable.
    Data,
    /// `F_BARRIERFSYNC`: everything already at `Data` on this device persists
    /// before any I/O that follows, with no claim about when.
    Fence,
    /// `F_FULLFSYNC`: "data that had been fsync'd on the same device before is
    /// guaranteed to be persisted when this call returns".
    Flush,
}

/// Process-wide sync tally. Instrumentation for tests and the phase example;
/// no engine decision reads it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceSyncCounts {
    pub data: u64,
    pub fences: u64,
    pub flushes: u64,
}

impl std::ops::Sub for DeviceSyncCounts {
    type Output = Self;
    fn sub(self, earlier: Self) -> Self {
        Self {
            data: self.data - earlier.data,
            fences: self.fences - earlier.fences,
            flushes: self.flushes - earlier.flushes,
        }
    }
}

static DATA: AtomicU64 = AtomicU64::new(0);
static FENCES: AtomicU64 = AtomicU64::new(0);
static FLUSHES: AtomicU64 = AtomicU64::new(0);

/// Every device sync soopy issued since process start, by level.
pub fn device_sync_counts() -> DeviceSyncCounts {
    DeviceSyncCounts {
        data: DATA.load(Ordering::Relaxed),
        fences: FENCES.load(Ordering::Relaxed),
        flushes: FLUSHES.load(Ordering::Relaxed),
    }
}

/// Per-phase sync accounting for the tracing spans.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SyncMeter {
    data: u64,
    fences: u64,
    flushes: u64,
    nanos: u128,
}

impl SyncMeter {
    pub(crate) fn file(&mut self, file: &File, level: SyncLevel) -> io::Result<()> {
        let started = Instant::now();
        let outcome = sync_now(file, level);
        self.nanos += started.elapsed().as_nanos();
        let (local, global) = match level {
            SyncLevel::Data => (&mut self.data, &DATA),
            SyncLevel::Fence => (&mut self.fences, &FENCES),
            SyncLevel::Flush => (&mut self.flushes, &FLUSHES),
        };
        *local += 1;
        global.fetch_add(1, Ordering::Relaxed);
        outcome
    }

    /// Syncs a directory, which is what carries the names created inside it.
    pub(crate) fn directory(&mut self, path: &Path, level: SyncLevel) -> io::Result<()> {
        let file = File::open(path)?;
        self.file(&file, level)
    }

    pub(crate) fn data(self) -> u64 {
        self.data
    }

    pub(crate) fn fences(self) -> u64 {
        self.fences
    }

    pub(crate) fn flushes(self) -> u64 {
        self.flushes
    }

    pub(crate) fn millis(self) -> f64 {
        self.nanos as f64 / 1_000_000.0
    }
}

/// Publishes `bytes` at `path` through a same-directory temporary, syncing the
/// body to `level` before the rename. The caller owns the parent directory sync.
pub(crate) fn publish_file(
    path: &Path,
    bytes: &[u8],
    level: SyncLevel,
    meter: &mut SyncMeter,
    prepare: impl FnOnce(&File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = temporary_in(parent)?;
    temporary.write_all(bytes)?;
    prepare(temporary.as_file())?;
    meter.file(temporary.as_file(), level)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(unix)]
fn temporary_in(parent: &Path) -> io::Result<NamedTempFile> {
    use std::os::unix::fs::PermissionsExt;
    // open(2) masks 0o666 with the umask, keeping the mode a plain create takes.
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o666))
        .tempfile_in(parent)
}

#[cfg(not(unix))]
fn temporary_in(parent: &Path) -> io::Result<NamedTempFile> {
    tempfile::Builder::new().tempfile_in(parent)
}

#[cfg(target_os = "macos")]
fn sync_now(file: &File, level: SyncLevel) -> io::Result<()> {
    use nix::errno::Errno;
    use nix::fcntl::{fcntl, FcntlArg};

    if level == SyncLevel::Data {
        return nix::unistd::fsync(file).map_err(io::Error::from);
    }
    let argument = match level {
        SyncLevel::Fence => FcntlArg::F_BARRIERFSYNC,
        _ => FcntlArg::F_FULLFSYNC,
    };
    match fcntl(file, argument) {
        Ok(_) => Ok(()),
        // A volume without the barrier command drains instead, so ordering the
        // protocol asked for is never quietly dropped.
        Err(Errno::ENOTTY | Errno::ENOTSUP | Errno::EINVAL) if level == SyncLevel::Fence => {
            file.sync_all()
        }
        Err(error) => Err(io::Error::from(error)),
    }
}

#[cfg(not(target_os = "macos"))]
fn sync_now(file: &File, level: SyncLevel) -> io::Result<()> {
    // Off Apple, fsync is both the fence and the durability point.
    match level {
        SyncLevel::Data => file.sync_data(),
        _ => file.sync_all(),
    }
}

/// Stamps a phase span with its sync tally and wall time.
pub(crate) fn record_sync(span: &tracing::Span, meter: SyncMeter, started: Instant) {
    span.record("sync.data", meter.data());
    span.record("sync.fences", meter.fences());
    span.record("sync.flushes", meter.flushes());
    span.record("sync_ms", meter.millis());
    span.record("duration_ms", elapsed_millis(started));
}

pub(crate) fn elapsed_millis(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}
