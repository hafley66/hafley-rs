// scaffolding: pending wire-or-delete ruling — task G1 built, no caller yet (see report table)
#![allow(dead_code)]

//! GIF background: decode + game-time frame clock. Pure functions, no Godot objects — the
//! background node (later task) owns turning `GifFrame::rgba` into an `ImageTexture` and
//! calling `advance`/`frozen` each `_process`. See plans/gif-background-library.md, task G1.
//!
//! Policy pins (plan §"Test contracts", finalized 2026-07-02): reject a gif whose width or
//! height exceeds `MAX_DIM_PX`, or whose byte length exceeds `MAX_BYTES` (= ASSET_MAX,
//! world-protocol.md §T) — reject, never downscale. Decode never panics: anything that isn't
//! a well-formed gif comes back as `Err`.
//!
//! §3 game-time coupling: the gif must freeze with hitlag, not run on wall-clock. `advance`
//! takes `frozen` from the caller each step; `frozen(state)` derives it from `SimState` so the
//! shell can gate the clock without the gif module knowing anything about fighters beyond the
//! one field it reads.

use crate::sim::SimState;

/// Reject a gif whose width or height (in pixels) exceeds this. A large gif decodes to many
/// full RGBA buffers held in RAM for the whole session; capped rather than downscaled (plan:
/// "revisit if it annoys").
pub const MAX_DIM_PX: u16 = 1024;

/// Reject a gif whose encoded byte length exceeds this. Matches `ASSET_MAX`
/// (world-protocol.md §T) since an imported gif becomes a world asset blob.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// One decoded frame: raw RGBA8 pixels (`width * height * 4` bytes, row-major, no padding) plus
/// how long it holds before the next frame. No `ImageTexture` here — that conversion is the
/// Godot node's job (later task), so this stays testable without a running engine.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GifFrame {
    pub rgba: Vec<u8>,
    pub delay_ms: u32,
}

/// A fully-decoded gif: dims (shared by every frame — the plan's fit mode is "cover", so the
/// background node scales the whole animation as one image) plus the frame list.
#[derive(Clone, PartialEq, Debug)]
pub struct GifAnim {
    pub width: u16,
    pub height: u16,
    pub frames: Vec<GifFrame>,
}

/// Why `decode_gif` refused the bytes. Never a panic — always one of these.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GifErr {
    /// Empty input.
    Empty,
    /// Width or height over `MAX_DIM_PX`, or byte length over `MAX_BYTES`.
    TooBig,
    /// Not a gif, truncated, or otherwise malformed. Carries the decoder's message.
    Invalid(String),
}

/// Decode `bytes` into frames + delays + dims. Pure: no filesystem, no Godot. Cheap policy
/// checks (empty, byte cap) run before the decoder even opens the stream; the dimension cap
/// runs right after the header, before any frame's pixels are decoded.
pub fn decode_gif(bytes: &[u8]) -> Result<GifAnim, GifErr> {
    if bytes.is_empty() {
        return Err(GifErr::Empty);
    }
    if bytes.len() > MAX_BYTES {
        return Err(GifErr::TooBig);
    }

    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = opts
        .read_info(bytes)
        .map_err(|e| GifErr::Invalid(e.to_string()))?;

    let width = decoder.width();
    let height = decoder.height();
    if width > MAX_DIM_PX || height > MAX_DIM_PX {
        return Err(GifErr::TooBig);
    }

    let mut frames = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| GifErr::Invalid(e.to_string()))?
    {
        frames.push(GifFrame {
            rgba: frame.buffer.to_vec(),
            delay_ms: u32::from(frame.delay) * 10, // gif delay unit is centiseconds
        });
    }
    if frames.is_empty() {
        return Err(GifErr::Invalid("gif has no frames".to_string()));
    }

    Ok(GifAnim {
        width,
        height,
        frames,
    })
}

/// Playhead into a `GifAnim`: which frame is showing, plus how far into that frame's delay.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct AnimPos {
    pub frame: usize,
    pub elapsed_ms: u32,
}

/// Pure frame clock. `frozen` holds the position exactly where it is (hitlag: the gif pops
/// with the fighters). Unfrozen, `dt_ms` accumulates into the current frame's delay and steps
/// forward one or more frames as delays elapse, wrapping at the end of the loop. A frame with
/// `delay_ms == 0` (some encoders emit these) is floored to 1ms so the loop always makes
/// progress instead of spinning forever on a zero-length hold.
pub fn advance(anim: &GifAnim, pos: AnimPos, dt_ms: u32, frozen: bool) -> AnimPos {
    if frozen || anim.frames.is_empty() {
        return pos;
    }
    let n = anim.frames.len();
    let mut frame = pos.frame % n;
    let mut elapsed = pos.elapsed_ms + dt_ms;
    loop {
        let delay = anim.frames[frame].delay_ms.max(1);
        if elapsed < delay {
            break;
        }
        elapsed -= delay;
        frame = (frame + 1) % n;
    }
    AnimPos {
        frame,
        elapsed_ms: elapsed,
    }
}

/// The gif is cosmetic (shell-side), so it gates on the rendered fighters rather than being
/// sim state itself: frozen the instant any live fighter is in hitlag, resumed the frame every
/// live fighter's hitlag has ticked back to 0. `state.fighters[..active]` only — dormant slots
/// hold a valid but unstepped `Fighter` and must not be able to freeze the background.
pub fn frozen(state: &SimState) -> bool {
    state.fighters[..state.active as usize]
        .iter()
        .any(|f| f.hitlag > 0)
}

#[cfg(test)]
mod tests {
    use super::{AnimPos, GifErr, advance, decode_gif, frozen};
    use crate::sim::SimState;

    // 2x2px, 2-frame animated gif (NETSCAPE2.0 loop ext), frame0 red @100ms, frame1 blue @200ms.
    // Produced with Pillow (`Image.save(..., format="GIF", save_all=True, duration=[100, 200])`)
    // and inlined as bytes here so the test needs no filesystem/fixture asset.
    #[rustfmt::skip]
    const TWO_FRAME_GIF: [u8; 111] = [
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x02, 0x00, 0x02, 0x00, 0x81, 0x00, 0x00, 0xff, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x21, 0xff, 0x0b, 0x4e, 0x45, 0x54, 0x53,
        0x43, 0x41, 0x50, 0x45, 0x32, 0x2e, 0x30, 0x03, 0x01, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x08,
        0x0a, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x08, 0x06,
        0x00, 0x01, 0x08, 0x04, 0x10, 0x10, 0x00, 0x21, 0xf9, 0x04, 0x08, 0x14, 0x00, 0x00, 0x00, 0x2c,
        0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x81, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x06, 0x00, 0x01, 0x08, 0x04, 0x10, 0x10, 0x00, 0x3b,
    ];

    // 1025x1px single-frame gif (Pillow, solid color) — over MAX_DIM_PX on width.
    #[rustfmt::skip]
    const OVERSIZE_DIM_GIF: [u8; 92] = [
        0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x01, 0x04, 0x01, 0x00, 0x81, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x04,
        0x01, 0x00, 0x00, 0x08, 0x35, 0x00, 0x01, 0x08, 0x1c, 0x48, 0xb0, 0xa0, 0xc1, 0x83, 0x08, 0x13,
        0x2a, 0x5c, 0xc8, 0xb0, 0xa1, 0xc3, 0x87, 0x10, 0x23, 0x4a, 0x9c, 0x48, 0xb1, 0xa2, 0xc5, 0x8b,
        0x18, 0x33, 0x6a, 0xdc, 0xc8, 0xb1, 0xa3, 0xc7, 0x8f, 0x20, 0x43, 0x8a, 0x1c, 0x49, 0xb2, 0xa4,
        0xc9, 0x93, 0x28, 0x53, 0xaa, 0x5c, 0xc9, 0x72, 0x64, 0x40, 0x00, 0x3b,
    ];

    // 1. decode_gif on the 2-frame fixture: 2 frames, each frame's delay_ms matches, dims match.
    #[test]
    fn decodes_two_frame_fixture() {
        let anim = decode_gif(&TWO_FRAME_GIF).expect("valid gif decodes");
        assert_eq!(anim.width, 2);
        assert_eq!(anim.height, 2);
        assert_eq!(anim.frames.len(), 2);
        assert_eq!(anim.frames[0].delay_ms, 100);
        assert_eq!(anim.frames[1].delay_ms, 200);
        for f in &anim.frames {
            assert_eq!(f.rgba.len(), 2 * 2 * 4);
        }
    }

    // 2. non-gif bytes -> Err, no panic; empty slice -> Err.
    #[test]
    fn rejects_non_gif_and_empty() {
        assert_eq!(decode_gif(&[]), Err(GifErr::Empty));
        match decode_gif(b"not a gif at all") {
            Err(GifErr::Invalid(_)) => {}
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // 3. a gif over the dim cap -> Err(TooBig).
    #[test]
    fn rejects_oversize_dims() {
        assert_eq!(decode_gif(&OVERSIZE_DIM_GIF), Err(GifErr::TooBig));
    }

    // 4. advance: frozen spans hold, unfrozen spans step frames on their delays, position
    // wraps at the loop end.
    #[test]
    fn clock_holds_when_frozen_and_wraps_on_loop() {
        let anim = decode_gif(&TWO_FRAME_GIF).expect("valid gif decodes");
        let mut pos = AnimPos::default(); // frame 0, elapsed 0

        // Unfrozen, short of frame 0's 100ms delay: stays on frame 0, accumulates elapsed.
        pos = advance(&anim, pos, 50, false);
        assert_eq!(
            pos,
            AnimPos {
                frame: 0,
                elapsed_ms: 50
            }
        );

        // Frozen: dt is thrown away entirely, position does not move.
        pos = advance(&anim, pos, 60, true);
        assert_eq!(
            pos,
            AnimPos {
                frame: 0,
                elapsed_ms: 50
            }
        );

        // Unfrozen again: 50 + 60 = 110ms crosses frame 0's 100ms delay -> frame 1, elapsed 10.
        pos = advance(&anim, pos, 60, false);
        assert_eq!(
            pos,
            AnimPos {
                frame: 1,
                elapsed_ms: 10
            }
        );

        // 10 + 250 = 260ms crosses frame 1's 200ms delay, wraps back to frame 0 with 60ms left.
        pos = advance(&anim, pos, 250, false);
        assert_eq!(
            pos,
            AnimPos {
                frame: 0,
                elapsed_ms: 60
            }
        );
    }

    // 5. freeze predicate: frozen(state) == any live fighter's hitlag > 0, against a SimState
    // fixture with one fighter in hitlag, and against none.
    #[test]
    fn freeze_predicate_reads_live_fighter_hitlag() {
        let calm = SimState::spawn();
        assert!(!frozen(&calm));

        let mut popped = calm;
        popped.fighters[0].hitlag = 5;
        assert!(frozen(&popped));

        // A dormant slot (>= active) in hitlag must not freeze the background.
        let mut dormant_only = calm;
        assert!(dormant_only.active < crate::sim::MAX_PLAYERS as u8);
        dormant_only.fighters[dormant_only.active as usize].hitlag = 5;
        assert!(!frozen(&dormant_only));
    }
}
