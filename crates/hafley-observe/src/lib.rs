#[path = "0_types.rs"]
mod _0_types;
#[path = "1_format.rs"]
mod _1_format;
#[path = "1_init.rs"]
mod _1_init;
#[cfg(all(feature = "otlp", not(target_arch = "wasm32")))]
#[path = "2_otlp.rs"]
mod _2_otlp;

#[cfg(all(feature = "otlp", target_arch = "wasm32"))]
compile_error!("hafley-observe `otlp` requires a native target");

pub use _0_types::{Config, OutputFormat, ParseOutputFormatError};
pub use _1_format::{env_filter, format_layer, FormatConfig};
pub use _1_init::{init, init_with_writer, startup};
#[cfg(all(feature = "otlp", not(target_arch = "wasm32")))]
pub use _2_otlp::{otlp_layer, otlp_provider, OtlpConfig, OtlpError, DEFAULT_OTLP_ENDPOINT};
