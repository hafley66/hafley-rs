// Ported from hafley-rs-game-runtime crates/input at 8646fa2 (MIT OR Apache-2.0).
use serde::{Deserialize, Serialize};
#[derive(Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub axes: [i8; 4],
    pub buttons: u32,
}
