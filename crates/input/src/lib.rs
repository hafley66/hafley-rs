//! Input values and snapshot-owned input buffering, independent of game/engine IO.
#[path = "0_types.rs"]
mod types;
#[path = "1_quantize.rs"]
mod quantize;
#[path = "2_buffer.rs"]
mod buffer;
pub use types::PlayerInput;
pub use quantize::{dequantize_axis, quantize_axis};
pub use buffer::{Buffer, Outcome};
