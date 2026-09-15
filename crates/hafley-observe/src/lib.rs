#[path = "0_types.rs"]
mod _0_types;
#[path = "1_format.rs"]
mod _1_format;
#[path = "1_init.rs"]
mod _1_init;
#[path = "2_otlp.rs"]
mod _2_otlp;

pub use _0_types::{Config, OutputFormat, ParseOutputFormatError};
pub use _1_format::{env_filter, format_layer, FormatConfig};
pub use _1_init::{init, init_with_writer, startup};
pub use _2_otlp::shutdown;

pub(crate) use _2_otlp::otlp_layer;
