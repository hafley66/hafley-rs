//! ScriptedDevice: tick-space input scripting (plans/e2e-playwright.md, "Scripted inputs in
//! tick-space"). A script is a list of fixed-duration impulses keyed on absolute sim tick, not
//! wall-clock -- so a scripted press is exactly N ticks of game time regardless of frame-pacing
//! jitter, and Playwright's wall-clock keyboard events never have to sample a sim tick boundary.
//!
//! This module is the pure half: a script entry and the lookup that turns (script, tick) into an
//! optional `InputFrame`. The impure half (parsing `?script=` off the boot query string, storing
//! the installed script, and calling in at the local-input site) lives in `crate::webtest`, which
//! is also where the substitution rides the normal input pipe -- ggrs never knows a frame came
//! from a script instead of a device, so two scripted pages are a real networked fight and a
//! checksum-parity assertion over them is a real desync canary.

use crate::sim::InputFrame;

/// One scripted impulse: `frame` is held for `duration_ticks` ticks starting at `start_tick`
/// (inclusive of `start_tick`, exclusive of `start_tick + duration_ticks`).
#[derive(Clone, Copy)]
pub struct ScriptEntry {
    pub start_tick: u64,
    pub duration_ticks: u64,
    pub frame: InputFrame,
}

impl ScriptEntry {
    fn covers(&self, tick: u64) -> bool {
        tick >= self.start_tick && tick < self.start_tick.saturating_add(self.duration_ticks)
    }
}

/// Pure lookup: the scripted frame for `tick`, or `None` if no entry covers it -- the caller then
/// falls back to the real device poll. Later entries win on overlap (last match in scan order), so
/// a short correction can be layered on top of a longer hold without trimming the earlier entry.
pub fn scripted_frame(script: &[ScriptEntry], tick: u64) -> Option<InputFrame> {
    script
        .iter()
        .rev()
        .find(|e| e.covers(tick))
        .map(|e| e.frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with_dir(dir: f32) -> InputFrame {
        InputFrame {
            dir,
            ..Default::default()
        }
    }

    #[test]
    fn empty_script_never_covers() {
        assert!(scripted_frame(&[], 0).is_none());
    }

    #[test]
    fn covers_inclusive_start_exclusive_end() {
        let e = ScriptEntry {
            start_tick: 10,
            duration_ticks: 5,
            frame: frame_with_dir(1.0),
        };
        let script = [e];
        assert!(scripted_frame(&script, 9).is_none());
        assert!(scripted_frame(&script, 10).is_some());
        assert!(scripted_frame(&script, 14).is_some());
        assert!(scripted_frame(&script, 15).is_none());
    }

    #[test]
    fn zero_duration_covers_nothing() {
        let e = ScriptEntry {
            start_tick: 10,
            duration_ticks: 0,
            frame: frame_with_dir(1.0),
        };
        assert!(scripted_frame(&[e], 10).is_none());
    }

    #[test]
    fn later_overlapping_entry_wins() {
        let a = ScriptEntry {
            start_tick: 0,
            duration_ticks: 20,
            frame: frame_with_dir(1.0),
        };
        let b = ScriptEntry {
            start_tick: 5,
            duration_ticks: 5,
            frame: frame_with_dir(-1.0),
        };
        let script = [a, b];
        assert_eq!(scripted_frame(&script, 3).unwrap().dir, 1.0);
        assert_eq!(scripted_frame(&script, 7).unwrap().dir, -1.0); // b wins in [5,10)
        assert_eq!(scripted_frame(&script, 12).unwrap().dir, 1.0); // back to a past b's window
    }

    #[test]
    fn tick_before_or_after_every_entry_falls_through() {
        let e = ScriptEntry {
            start_tick: 100,
            duration_ticks: 1,
            frame: frame_with_dir(1.0),
        };
        assert!(scripted_frame(&[e], 50).is_none());
        assert!(scripted_frame(&[e], 200).is_none());
    }

    #[test]
    fn duration_saturates_instead_of_overflowing() {
        // start_tick + duration_ticks would overflow u64; `covers` must saturate instead of panicking
        // (debug builds panic on arithmetic overflow), and the saturated end is still exclusive.
        let e = ScriptEntry {
            start_tick: u64::MAX - 2,
            duration_ticks: u64::MAX,
            frame: frame_with_dir(1.0),
        };
        assert!(scripted_frame(&[e], u64::MAX - 1).is_some());
        assert!(scripted_frame(&[e], u64::MAX).is_none());
    }
}
