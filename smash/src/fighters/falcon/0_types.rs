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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attack {
    pub id: u8,
    pub position: [f32; 3],
    pub radius: f32,
    pub enabled: bool,
    pub aerial: bool,
    pub damage: f32,
    pub kbg: u32,
    pub bkb: u32,
    pub wdsk: u32,
    pub trajectory: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub interruptible: bool,
    pub landing_lag: bool,
    pub x_pos: f32,
    pub y_pos: f32,
    pub hit_boxes: Vec<Attack>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub iasa: Option<usize>,
    pub landing_lag: Option<f32>,
    pub frames: Vec<Frame>,
}
