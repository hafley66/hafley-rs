//! Request-scoped filesystem base. Fact paths retain their caller spelling.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
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
