#[path = "0_types.rs"]
mod types;
pub use types::*;

#[cfg(feature = "ingest")]
#[path = "1_decode.rs"]
mod decode;
#[cfg(feature = "ingest")]
pub use decode::{decode_file, decode_html};

#[cfg(feature = "ingest")]
#[path = "2_bake.rs"]
mod bake;
#[cfg(feature = "ingest")]
pub use bake::bake;
