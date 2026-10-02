//! Test path overrides are local to the calling thread, never process env.

#[cfg(any(test, feature = "testing"))]
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[cfg(any(test, feature = "testing"))]
thread_local! {
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(any(test, feature = "testing"))]
pub fn set_root(root: &Path) {
    ROOT.with(|value| *value.borrow_mut() = Some(root.to_path_buf()));
}

#[cfg(any(test, feature = "testing"))]
pub fn root() -> Option<PathBuf> {
    ROOT.with(|value| value.borrow().clone())
}

#[cfg(any(test, feature = "testing"))]
pub fn spawn<F, T>(work: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let inherited = root();
    std::thread::spawn(move || {
        if let Some(root) = inherited {
            set_root(&root);
        }
        work()
    })
}

/// Resolve the account home independently of HOME, including in sandboxed tests.
#[cfg(all(unix, any(test, feature = "testing")))]
pub fn account_home() -> PathBuf {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;
    let mut buffer = vec![0u8; 16384];
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found = std::ptr::null_mut();
        // getpwuid_r writes into entry and buffer; pw_dir remains valid until
        // buffer is dropped. Copy the path before returning.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if status == libc::ERANGE {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        assert!(
            status == 0 && !found.is_null(),
            "resolve account home for test path guard"
        );
        let directory = unsafe { CStr::from_ptr((*found).pw_dir) };
        return PathBuf::from(OsStr::from_bytes(directory.to_bytes()));
    }
}

#[cfg(all(not(unix), any(test, feature = "testing")))]
pub fn account_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .expect("resolve account home for test path guard")
}

#[cfg(any(test, feature = "testing"))]
pub fn guard(path: &Path) {
    static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let home = HOME.get_or_init(account_home);
    let resolved = path
        .ancestors()
        .find_map(|ancestor| {
            ancestor
                .canonicalize()
                .ok()
                .map(|base| base.join(path.strip_prefix(ancestor).unwrap()))
        })
        .unwrap_or_else(|| path.to_path_buf());
    for suffix in [
        ".agent",
        ".cache/boop",
        ".config/boop",
        "Library/Application Support/boop",
    ] {
        let protected = home.join(suffix);
        assert!(
            !path.starts_with(&protected) && !resolved.starts_with(&protected),
            "test resolved real home Boop path: {}",
            path.display()
        );
    }
}

#[test]
fn resolving_real_home_store_mail_and_trails_panics_before_io() {
    let home = account_home();
    set_root(&home);
    let panic = std::panic::catch_unwind(crate::Store::default_path).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<String>().unwrap(),
        &format!(
            "test resolved real home Boop path: {}",
            home.join(".agent/boop.db").display()
        )
    );
    for path in [
        home.join(".agent/boop.db"),
        home.join(".agent/mail"),
        home.join(".agent/lanes"),
        home.join(".cache/boop/lanes"),
    ] {
        let panic = std::panic::catch_unwind(|| guard(&path)).unwrap_err();
        let message = panic.downcast_ref::<String>().unwrap();
        assert_eq!(
            message,
            &format!("test resolved real home Boop path: {}", path.display())
        );
    }
    ROOT.with(|value| *value.borrow_mut() = None);
}

#[cfg(not(any(test, feature = "testing")))]
pub fn root() -> Option<PathBuf> {
    None
}

#[cfg(not(any(test, feature = "testing")))]
pub fn guard(_path: &Path) {}

pub fn guard_default(path: &Path) {
    guard(path);
    #[cfg(any(test, feature = "testing"))]
    assert!(
        root().is_some() || std::env::var_os("BOOP_TEST_STRICT_PATHS").is_none(),
        "test resolved unscoped default Boop path: {}",
        path.display()
    );
}
