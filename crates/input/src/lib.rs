//! Device-independent values carried across deterministic simulation frames.

#[path = "1_quantize.rs"]
mod quantize;
#[path = "0_types.rs"]
mod types;

pub use quantize::{dequantize_axis, quantize_axis};
pub use types::PlayerInput;
