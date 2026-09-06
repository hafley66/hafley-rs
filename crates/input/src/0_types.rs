use serde::{Deserialize, Serialize};

/// Compact input sampled once for one participant during one simulation frame.
///
/// Axis meaning and button-bit assignments belong to the consuming game. Keeping the wire value
/// free of action names lets native, web, engine, replay, and rollback adapters share its layout.
#[derive(Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub axes: [i8; 4],
    pub buttons: u32,
}
