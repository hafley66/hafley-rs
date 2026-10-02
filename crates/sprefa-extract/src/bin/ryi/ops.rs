use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crate::cli_auto::{Cmd, Ryi};
use crate::models::file_args::FileArgs;
use crate::ops_auto::{
    CapabilitiesArgs, CleaveArgs, DiffArgs, ExtractArgs, FastArgs, GraphArgs, IngestArgs, MoveArgs,
    OpError, OpResult, QueryArgs, RegionArgs, RenameArgs, SchemaArgs, ScipArgs, SlowArgs,
    StratifyArgs, TrailArgs, WatchArgs,
};

fn command(cmd: Cmd) -> Ryi {
    Ryi {
        cmd: Some(cmd),
        file: FileArgs::default(),
    }
}

struct RowSink {
    tx: mpsc::SyncSender<OpResult<Vec<u8>>>,
    pending: Vec<u8>,
    chunked: bool,
}

impl RowSink {
    fn flush_pending(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            self.tx
                .send(Ok(std::mem::take(&mut self.pending)))
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        }
        Ok(())
    }
}

impl Write for RowSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.chunked {
            let mut remaining = buf;
            while let Some(end) = remaining.iter().position(|byte| *byte == b'\n') {
                self.pending.extend_from_slice(&remaining[..=end]);
                remaining = &remaining[end + 1..];
                self.flush_pending()?;
            }
            self.pending.extend_from_slice(remaining);
            return Ok(buf.len());
        }
        let mut remaining = buf;
        while !remaining.is_empty() {
            let size = (64 * 1024 - self.pending.len()).min(remaining.len());
            self.pending.extend_from_slice(&remaining[..size]);
            remaining = &remaining[size..];
            if self.pending.len() == 64 * 1024 {
                self.tx
                    .send(Ok(std::mem::take(&mut self.pending)))
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.chunked {
            Ok(())
        } else {
            self.flush_pending()
        }
    }
}

#[derive(Clone)]
struct SharedSink(Arc<Mutex<RowSink>>);

impl Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

thread_local! {
    static OP_SINK: RefCell<Option<SharedSink>> = const { RefCell::new(None) };
    static OP_WRITE_ERROR: RefCell<Option<std::io::Error>> = const { RefCell::new(None) };
    static REQUEST_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static REQUEST_DIAGNOSTICS: RefCell<Option<Diagnostics>> = const { RefCell::new(None) };
    static REQUEST_INPUT: RefCell<Option<Arc<tempfile::NamedTempFile>>> = const { RefCell::new(None) };
}

pub(crate) type Diagnostics = Arc<Mutex<Vec<u8>>>;

struct RequestContextGuard {
    root: Option<PathBuf>,
    diagnostics: Option<Diagnostics>,
}

impl Drop for RequestContextGuard {
    fn drop(&mut self) {
        REQUEST_ROOT.with(|slot| {
            slot.replace(self.root.take());
        });
        REQUEST_DIAGNOSTICS.with(|slot| {
            slot.replace(self.diagnostics.take());
        });
    }
}

pub(crate) fn with_request_context<T>(
    root: PathBuf,
    diagnostics: Option<Diagnostics>,
    run: impl FnOnce() -> T,
) -> T {
    let library_diagnostics = diagnostics.clone();
    let guard = RequestContextGuard {
        root: REQUEST_ROOT.with(|slot| slot.replace(Some(root.clone()))),
        diagnostics: REQUEST_DIAGNOSTICS.with(|slot| slot.replace(diagnostics)),
    };
    let result = sprefa_extract::with_diagnostic_sink(library_diagnostics, || {
        sprefa_extract::with_io_root(root, run)
    });
    drop(guard);
    result
}

struct RequestInputGuard(Option<Arc<tempfile::NamedTempFile>>);

impl Drop for RequestInputGuard {
    fn drop(&mut self) {
        REQUEST_INPUT.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

pub(crate) fn with_request_input<T>(
    input: Option<Arc<tempfile::NamedTempFile>>,
    run: impl FnOnce() -> T,
) -> T {
    let guard = RequestInputGuard(REQUEST_INPUT.with(|slot| slot.replace(input)));
    let result = run();
    drop(guard);
    result
}

pub(crate) fn request_input_file() -> Option<Arc<tempfile::NamedTempFile>> {
    REQUEST_INPUT.with(|slot| slot.borrow().clone())
}

pub(crate) fn print_diagnostic(args: std::fmt::Arguments<'_>) {
    let captured = REQUEST_DIAGNOSTICS.with(|slot| {
        let value = slot.borrow();
        value
            .as_ref()
            .map(|diagnostics| {
                let _ = writeln!(diagnostics.lock().unwrap(), "{args}");
            })
            .is_some()
    });
    if !captured {
        let _ = writeln!(std::io::stderr().lock(), "{args}");
    }
}

pub(crate) fn request_root() -> PathBuf {
    REQUEST_ROOT
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub(crate) fn print_line(args: std::fmt::Arguments<'_>) {
    OP_SINK.with(|slot| {
        if let Some(sink) = slot.borrow_mut().as_mut() {
            if let Err(error) = writeln!(sink, "{args}") {
                OP_WRITE_ERROR.with(|failure| *failure.borrow_mut() = Some(error));
            }
        } else {
            let _ = writeln!(std::io::stdout().lock(), "{args}");
        }
    });
}

pub(crate) fn flush_line() {
    OP_SINK.with(|slot| {
        if let Some(sink) = slot.borrow_mut().as_mut() {
            if let Err(error) = sink.flush() {
                OP_WRITE_ERROR.with(|failure| *failure.borrow_mut() = Some(error));
            }
        } else {
            let _ = std::io::stdout().flush();
        }
    });
}

struct Rows {
    rx: mpsc::Receiver<OpResult<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
}

impl Iterator for Rows {
    type Item = OpResult<Vec<u8>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

impl Drop for Rows {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn produce(ryi: Ryi) -> Rows {
    let (tx, rx) = mpsc::sync_channel(64);
    let cancelled = Arc::new(AtomicBool::new(false));
    let operation_cancelled = Arc::clone(&cancelled);
    let request_root = request_root();
    let diagnostics = REQUEST_DIAGNOSTICS.with(|slot| slot.borrow().clone());
    let request_input = request_input_file();
    let chunked = REQUEST_ROOT.with(|slot| slot.borrow().is_some())
        && !matches!(&ryi.cmd, Some(Cmd::Watch(_)));
    std::thread::spawn(move || {
        let result = with_request_input(request_input, || {
            with_request_context(request_root, diagnostics, || -> OpResult<()> {
                let sink = SharedSink(Arc::new(Mutex::new(RowSink {
                    tx: tx.clone(),
                    pending: Vec::new(),
                    chunked,
                })));
                OP_SINK.with(|slot| *slot.borrow_mut() = Some(sink.clone()));
                let outcome =
                    crate::run_verb(ryi, Box::new(sink.clone()), Some(operation_cancelled))
                        .map_err(|error| OpError(error.message, error.code));
                OP_SINK.with(|slot| *slot.borrow_mut() = None);
                sink.0.lock().unwrap().flush_pending()?;
                if let Some(error) = OP_WRITE_ERROR.with(|failure| failure.borrow_mut().take()) {
                    return Err(OpError::from(error));
                }
                outcome
            })
        });
        if let Err(error) = result {
            let _ = tx.send(Err(error));
        }
    });
    Rows { rx, cancelled }
}

fn stream(ryi: Ryi) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    Box::new(produce(ryi))
}

fn one(ryi: Ryi) -> OpResult<Vec<u8>> {
    let mut output = Vec::new();
    for line in produce(ryi) {
        output.extend(line?);
    }
    Ok(output)
}

pub fn fast(args: &FastArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Fast(args.clone())))
}
pub fn extract(args: &ExtractArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    file(&args.args)
}
pub fn file(args: &FileArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    let mut args = args.clone();
    args.format = None;
    stream(Ryi {
        cmd: None,
        file: args,
    })
}
pub fn slow(args: &SlowArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Slow(args.clone())))
}
pub fn scip(args: &ScipArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Scip(args.clone())))
}
pub fn graph(args: &GraphArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Graph(args.clone())))
}

pub fn stratify(args: &StratifyArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Stratify(args.clone())))
}
pub fn query(args: &QueryArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Query(args.clone())))
}
pub fn watch(args: &WatchArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Watch(args.clone())))
}
pub fn diff(args: &DiffArgs) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Diff(args.clone())))
}

pub fn cleave(args: &CleaveArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Cleave(args.clone())))
}
pub fn r#move(args: &MoveArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Move(args.clone())))
}
pub fn rename(args: &RenameArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Rename(args.clone())))
}
pub fn region(args: &RegionArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Region(args.clone())))
}
pub fn schema(_args: &SchemaArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Schema))
}
pub fn trail(args: &TrailArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Trail(args.clone())))
}

pub fn ingest(args: &IngestArgs) -> OpResult<Vec<u8>> {
    one(command(Cmd::Ingest(args.clone())))
}

pub fn capabilities(
    _args: &CapabilitiesArgs,
) -> Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send> {
    stream(command(Cmd::Capabilities))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_response_cancels_and_closes_its_row_sink() {
        let (tx, rx) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let rows = Rows {
            rx,
            cancelled: Arc::clone(&cancelled),
        };
        drop(rows);
        assert!(cancelled.load(Ordering::Acquire));
        let mut sink = RowSink {
            tx,
            pending: Vec::new(),
            chunked: false,
        };
        assert_eq!(
            sink.write_all(b"row\n").unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn daemon_rows_cross_the_channel_in_64_kib_chunks() {
        let (tx, rx) = mpsc::sync_channel(2);
        let mut sink = RowSink {
            tx,
            pending: Vec::new(),
            chunked: true,
        };
        for _ in 0..1023 {
            sink.write_all(&[b'x'; 64]).unwrap();
            sink.flush().unwrap();
        }
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        sink.write_all(&[b'x'; 64]).unwrap();
        assert_eq!(rx.try_recv().unwrap().unwrap().len(), 64 * 1024);
        sink.write_all(b"tail").unwrap();
        sink.flush().unwrap();
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        sink.flush_pending().unwrap();
        let tail = rx.try_recv().unwrap().unwrap();
        assert_eq!(tail.as_slice(), b"tail");
    }

    #[test]
    fn request_diagnostics_stay_in_the_request_buffer() {
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        with_request_context(PathBuf::from("/tmp"), Some(diagnostics.clone()), || {
            print_diagnostic(format_args!("0 facts: no extractor"));
        });
        assert_eq!(&*diagnostics.lock().unwrap(), b"0 facts: no extractor\n");
    }
}
