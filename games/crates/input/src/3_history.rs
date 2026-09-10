//! Tick-sampled device state. History belongs to the rollback snapshot.
use crate::PlayerInput;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    pub previous: PlayerInput,
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub held: PlayerInput,
    pub previous: PlayerInput,
    pub pressed: u32,
    pub released: u32,
}

impl History {
    pub fn advance(&mut self, held: PlayerInput) -> Frame {
        let frame = Frame {
            held,
            previous: self.previous,
            pressed: held.buttons & !self.previous.buttons,
            released: self.previous.buttons & !held.buttons,
        };
        self.previous = held;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_release_repress_and_restored_stick_crossing() {
        let tape = [(1, 127), (1, 127), (0, 0), (5, -127), (4, -127)];
        let mut history = History::default();
        let mut snapshot = history;
        let mut observed = Vec::new();
        for (tick, (buttons, axis)) in tape.into_iter().enumerate() {
            if tick == 2 { snapshot = history; }
            let frame = history.advance(PlayerInput { buttons, axes: [axis, 0, 0, 0] });
            observed.push((frame.pressed, frame.released, frame.previous.axes[0], frame.held.axes[0]));
        }
        assert_eq!(observed, [(1, 0, 0, 127), (0, 0, 127, 127), (0, 1, 127, 0), (5, 0, 0, -127), (0, 1, -127, -127)]);
        for ((buttons, axis), expected) in tape[2..].iter().zip(&observed[2..]) {
            let frame = snapshot.advance(PlayerInput { buttons: *buttons, axes: [*axis, 0, 0, 0] });
            assert_eq!((frame.pressed, frame.released, frame.previous.axes[0], frame.held.axes[0]), *expected);
        }
        assert!(snapshot == history);
    }
}
