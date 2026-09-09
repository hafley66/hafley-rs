use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub action: usize,
    pub frame: usize,
    pub root: [f32; 3],
    pub damage: f32,
    pub contact: bool,
    pub hit: Option<(u8, f32)>,
}
pub use game_content::{Action, Attack, Frame};
