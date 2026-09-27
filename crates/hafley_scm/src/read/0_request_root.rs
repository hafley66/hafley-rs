//! Request-scoped filesystem base. Fact paths retain their caller spelling.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

thread_local! {
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static DIAGNOSTICS: RefCell<Option<Arc<Mutex<Vec<u8>>>>> = const { RefCell::new(None) };
}

pub fn with_diagnostic_sink<T>(sink: Option<Arc<Mutex<Vec<u8>>>>, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Mutex<Vec<u8>>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DIAGNOSTICS.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let restore = Restore(DIAGNOSTICS.with(|slot| slot.replace(sink)));
    let result = run();
    drop(restore);
    result
}

pub fn diagnostic_line(args: std::fmt::Arguments<'_>) {
    let captured = DIAGNOSTICS.with(|slot| {
        let sink = slot.borrow();
        sink.as_ref().is_some_and(|sink| {
            let mut bytes = sink.lock().unwrap_or_else(|poison| poison.into_inner());
            let _ = writeln!(bytes, "{args}");
            true
        })
    });
    if !captured {
        eprintln!("{args}");
    }
}

pub fn request_io_root() -> Option<PathBuf> {
    ROOT.with(|slot| slot.borrow().clone())
}

pub fn io_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        request_io_root().map_or_else(|| path.to_path_buf(), |root| root.join(path))
    }
}

pub fn with_io_root<T>(root: PathBuf, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ROOT.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let restore = Restore(ROOT.with(|slot| slot.replace(Some(root))));
    let result = run();
    drop(restore);
    result
}
