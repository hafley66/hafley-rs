//! The callbacks missing from rusqlite 0.40.2's virtual-table module builder.
//!
//! `Module<T>` is documented as `repr(transparent)` over `sqlite3_module` in
//! the pinned rusqlite version. The macro keeps the transmute at a concrete
//! table type, where Rust can prove both sides have the same size.

use rusqlite::ffi;
use std::ffi::{c_char, c_int};

#[derive(Clone, Copy)]
pub struct VtabCallbacks {
    pub rename: Option<unsafe extern "C" fn(*mut ffi::sqlite3_vtab, *const c_char) -> c_int>,
    pub shadow_name: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
    pub savepoint: Option<unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int>,
    pub release: Option<unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int>,
    pub rollback_to: Option<unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int>,
}

impl VtabCallbacks {
    pub const fn savepoints(
        savepoint: unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int,
        release: unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int,
        rollback_to: unsafe extern "C" fn(*mut ffi::sqlite3_vtab, c_int) -> c_int,
    ) -> Self {
        Self {
            rename: None,
            shadow_name: None,
            savepoint: Some(savepoint),
            release: Some(release),
            rollback_to: Some(rollback_to),
        }
    }

    pub const fn rename(
        mut self,
        callback: unsafe extern "C" fn(*mut ffi::sqlite3_vtab, *const c_char) -> c_int,
    ) -> Self {
        self.rename = Some(callback);
        self
    }

    pub const fn shadow_name(
        mut self,
        callback: unsafe extern "C" fn(*const c_char) -> c_int,
    ) -> Self {
        self.shadow_name = Some(callback);
        self
    }
}

/// Build a static-compatible rusqlite module with SQLite's optional callbacks.
///
/// Keep each callback's C unwind boundary guarded. `vtab_callback` provides
/// that guard for callbacks receiving a `sqlite3_vtab` pointer.
#[macro_export]
macro_rules! vtab_module {
    ($table:ty, $callbacks:expr) => {{
        const CALLBACKS: $crate::VtabCallbacks = $callbacks;
        const MODULE: $crate::rusqlite::vtab::Module<$table> = unsafe {
            let mut raw: $crate::rusqlite::ffi::sqlite3_module = std::mem::transmute(
                $crate::rusqlite::vtab::Module::<$table>::update_module_with_tx(),
            );
            raw.xRename = CALLBACKS.rename;
            raw.xShadowName = CALLBACKS.shadow_name;
            raw.xSavepoint = CALLBACKS.savepoint;
            raw.xRelease = CALLBACKS.release;
            raw.xRollbackTo = CALLBACKS.rollback_to;
            if CALLBACKS.shadow_name.is_some() {
                raw.iVersion = 3;
            } else if CALLBACKS.savepoint.is_some()
                || CALLBACKS.release.is_some()
                || CALLBACKS.rollback_to.is_some()
            {
                raw.iVersion = 2;
            }
            std::mem::transmute(raw)
        };
        MODULE
    }};
}

/// Convert a custom virtual-table C callback into SQLite's error convention.
///
/// # Safety
///
/// `raw` must point to a live `#[repr(C)]` table whose first field is
/// `sqlite3_vtab`. SQLite must own the call for its full duration.
#[tracing::instrument(level = "trace", skip_all, fields(callback))]
pub unsafe fn vtab_callback<T>(
    raw: *mut ffi::sqlite3_vtab,
    callback: &'static str,
    body: impl FnOnce(&mut T) -> rusqlite::Result<()>,
) -> c_int {
    tracing::Span::current().record("callback", callback);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let table = unsafe { &mut *raw.cast::<T>() };
        body(table)
    }))
    .unwrap_or_else(|_| Err(rusqlite::Error::ModuleError(format!("panic in {callback}"))));
    match result {
        Ok(()) => ffi::SQLITE_OK,
        Err(error) => {
            tracing::error!(%callback, %error, "virtual-table callback failed");
            unsafe { rusqlite::to_sqlite_error(&error, &mut (*raw).zErrMsg) }
        }
    }
}
