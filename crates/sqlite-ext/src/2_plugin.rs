//! The common registration path for linked and loadable SQLite plugins.

use hafley_observe::{Config, FormatConfig, OutputFormat};
use rusqlite::{Connection, Result};
use std::{io::IsTerminal, sync::Once};
use tracing_subscriber::{
    fmt::{format::FmtSpan, writer::BoxMakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static OBSERVE: Once = Once::new();

pub struct Plugin {
    pub name: &'static str,
    pub version: &'static str,
    pub default_filter: &'static str,
    pub install: fn(&Connection) -> Result<()>,
}

impl Plugin {
    pub const fn new(
        name: &'static str,
        version: &'static str,
        default_filter: &'static str,
        install: fn(&Connection) -> Result<()>,
    ) -> Self {
        Self {
            name,
            version,
            default_filter,
            install,
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(plugin = self.name, version = self.version))]
    pub fn register(&self, db: &Connection) -> Result<()> {
        self.observe(db);
        (self.install)(db)
    }

    fn observe(&self, db: &Connection) {
        OBSERVE.call_once(|| {
            let ansi = std::io::stderr().is_terminal();
            let config = Config::from_env(self.name, self.version, self.default_filter, ansi)
                .unwrap_or(Config {
                    service_name: self.name,
                    service_version: self.version,
                    default_filter: self.default_filter,
                    format: OutputFormat::Human,
                    ansi,
                });
            let format = FormatConfig {
                span_events: FmtSpan::NEW | FmtSpan::CLOSE,
                ..FormatConfig::standard(config.format, config.ansi)
            };
            let installed = tracing_subscriber::registry()
                .with(hafley_observe::env_filter(config.default_filter))
                .with(hafley_observe::format_layer(
                    format,
                    BoxMakeWriter::new(std::io::stderr),
                ))
                .with(hafley_observe::chrome_layer())
                .try_init();
            if installed.is_ok() {
                hafley_observe::startup(&config);
            }
        });
        if hafley_observe::trace_path().is_some()
            || tracing::enabled!(target: hafley_observe::sqlite::SQLITE_TARGET, tracing::Level::DEBUG)
        {
            hafley_observe::sqlite::instrument(db);
        }
    }
}

/// Export a loadable extension entry point for a `Plugin` constant.
///
/// The same constant's `register` method is the linked, in-process path.
#[macro_export]
macro_rules! sqlite_extension {
    ($entry:ident, $plugin:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $entry(
            db: *mut $crate::rusqlite::ffi::sqlite3,
            error: *mut *mut std::os::raw::c_char,
            api: *mut $crate::rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int {
            fn register(db: $crate::rusqlite::Connection) -> $crate::rusqlite::Result<bool> {
                $plugin.register(&db)?;
                Ok(false)
            }
            unsafe { $crate::rusqlite::Connection::extension_init2(db, error, api, register) }
        }
    };
}
