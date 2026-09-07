//! Kneeman simulation, authored mechanics, replay, and rollback integration.
//! Godot device sampling and presentation live in the consuming application.

#[path = "0_input_frame.rs"]
pub mod input_frame;
#[path = "1_asset_wire.rs"]
pub mod asset_wire;
#[path = "2_netplay.rs"]
pub mod netplay;
#[path = "3_fixtures.rs"]
pub mod fixtures;
mod v1;
pub use v1::*;
