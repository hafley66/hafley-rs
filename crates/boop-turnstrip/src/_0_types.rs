//! The shapes the strip trades in, plus the two lookups the whole crate is
//! built on: `kind_of` (a harness role to a square kind) and `lines_of` (a
//! turn's own line count, which rides on its text and so costs no read).
//!
//! Field spelling is part of the contract, not a detail: the strip's shapes
//! cross JSON to a TypeScript client that already declares these names, so
//! every wire shape is camelCase (`#[serde(rename_all = "camelCase")]`) and
//! `TurnKind` is lowercase. The tables below are the one place where shapes are
//! not involved and where the TypeScript spelling is dropped.

use serde::{Deserialize, Serialize};

/// Which square a turn draws as. Serialized lowercase, the TypeScript client's
/// `"user" | "agent" | "tool" | "other"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnKind {
    User,
    Agent,
    Tool,
    Other,
}

/// `TurnKind` in the order every per-kind table in this crate is indexed by:
/// the TypeScript `KINDS` array, `user, agent, tool, other`. `TurnKind::index`
/// is the only place that order is written down.
pub const KINDS: [TurnKind; 4] = [
    TurnKind::User,
    TurnKind::Agent,
    TurnKind::Tool,
    TurnKind::Other,
];

/// A per-kind accumulator: `[f64; 4]` in `KINDS` order. The TypeScript
/// `zeroed()` record becomes this array, so a kind's slot is `kind.index()`.
pub const ZEROED: [f64; 4] = [0.0; 4];

impl TurnKind {
    /// This kind's slot in a `[f64; 4]` table: `user` 0, `agent` 1, `tool` 2,
    /// `other` 3 — the TypeScript `KINDS` order, and the index order for every
    /// accumulator in this crate.
    pub fn index(self) -> usize {
        match self {
            TurnKind::User => 0,
            TurnKind::Agent => 1,
            TurnKind::Tool => 2,
            TurnKind::Other => 3,
        }
    }
}

/// `Math.min(high, Math.max(low, value))`. NaN propagates, as it does in the
/// TypeScript; Rust's `f64::min`/`f64::max` would swallow it instead.
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    high.min(low.max(value))
}

/// A harness role to the square it draws as. Anything that is not a user turn,
/// a tool turn or an assistant turn is `other`.
pub fn kind_of(role: &str) -> TurnKind {
    if role == "user" {
        return TurnKind::User;
    }
    if role == "tool" {
        return TurnKind::Tool;
    }
    if role == "assistant" {
        return TurnKind::Agent;
    }
    TurnKind::Other
}

/// A turn's own lines, the projection's count: the text is already in memory.
/// Never below 1 — a turn with nothing in it still occupied a row.
pub fn lines_of(said: &str) -> i64 {
    1.max(said.matches('\n').count() as i64 + 1)
}

/// A viewport in buffer rows, inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub top: i64,
    pub bottom: i64,
}

/// One turn the strip draws, with what the matcher measured of it clipped to
/// the viewport. `start`/`end` are the turn's own buffer rows, unclipped — the
/// anchor the placement uses; `total` is its line count; `lines` is how many of
/// its lines landed on rows the viewport holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRow {
    pub id: String,
    pub kind: TurnKind,
    pub total: i64,
    pub start: i64,
    pub end: i64,
    pub lines: i64,
}

/// One turn of the session, as a recency list needs it: which turn it is and
/// what kind. No rows: the list draws places in the block, not rows, so a turn
/// the window lost is a member like any other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedTurn {
    pub id: String,
    pub kind: TurnKind,
}

/// The window's turns, oldest first, as the estimator wants them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowTurn {
    pub id: String,
    pub kind: TurnKind,
    pub total: i64,
}

/// What was measured for one turn, clipped to the viewport.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnSample {
    pub id: String,
    pub kind: TurnKind,
    /// The turn's own first buffer row, unclipped: the anchor placement uses.
    pub turn_start: i64,
    /// The rows of it the viewport holds, inclusive.
    pub start: i64,
    pub end: i64,
    /// The turn's own lines that landed inside the viewport.
    pub lines: i64,
    pub total: i64,
}

/// `kappa` and `gamma` from what is on screen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Estimates {
    /// Screen rows per logical line, per kind, in `KINDS` order. Never below 1:
    /// wrapping cannot use fewer rows than the turn has lines.
    pub kappa: [f64; 4],
    /// Rows nobody attributed between two adjacent sampled turns, per kind of
    /// the newer one, in `KINDS` order. Blank lines and separators live here.
    pub gamma: [f64; 4],
    /// The worst wrap this window measured, which is the clamp's ceiling.
    pub kappa_max: f64,
}

/// Every turn in the window, placed in buffer rows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub id: String,
    pub kind: TurnKind,
    /// Estimated first buffer row of the whole turn. `f64::NAN` until the
    /// placement passes have run — the TypeScript sentinel for "not placed
    /// yet", kept as the same NaN.
    pub start: f64,
    /// Estimated buffer rows for the whole turn, seen or not.
    pub rows: f64,
    pub total: i64,
    /// Fraction of the turn inside the viewport; 1 when nothing was measured.
    /// `0 < seen < 1` is a turn the reader is halfway through.
    pub seen: f64,
    pub measured: bool,
    /// The buffer rows the matcher actually saw on screen, inclusive — the
    /// turn's own span clipped to the viewport. `None` when nothing was
    /// measured. This is the extent a strip that draws on the reader's rows
    /// needs: `start`/`rows` describe the whole turn, estimated, so a turn whose
    /// head scrolled off the top ends above the window in that space while it is
    /// plainly on screen.
    pub visible: Option<(f64, f64)>,
}

/// Which placement the strip draws.
///
/// Two answers to "where does a square go", and they disagree about what the
/// strip *is*:
///
///   - [`Mode::Relative`]: the strip is the reader's window. A square sits on
///     the row its turn starts on, so a scroll moves every square with the text
///     and a turn whose head scrolled off the top draws at row 0. Only a turn
///     the matcher saw on these rows has a row to name, so a turn above the
///     window draws no square at all — except the reader's own, which the pinned
///     band keeps.
///   - [`Mode::Recent`]: the strip is the newest turns of the session, one
///     square each, uniform, oldest first; `y` counts places in the block, no
///     row and no span, so a scroll moves nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Relative,
    Recent,
}

/// Pinned by measurement on a busy pane, not by taste. The crate has no pixel
/// constants in either mode: a caller draws `y * cell_height` (relative) or
/// steps `y` along its own track, one square per step (recent).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub mode: Mode,
    /// The floor a square's size may fall to. A square is as big as the part of
    /// its turn the reader can see ([`Square::scale`]), and a sliver of a turn
    /// clamps here rather than shrinking away, so a turn the window barely holds
    /// stays findable.
    pub scale_min: f64,
    /// Least distance between two squares, in window rows. A square draws
    /// smaller than one row, so `1` keeps adjacent rows from touching. Relative
    /// mode only: a recent strip has no rows to collide on.
    pub min_gap: f64,
    /// The strip's own budget: at most this many squares. `0` leaves the mode's
    /// own bound — one square per window row in relative mode, and
    /// [`DEFAULT_RECENT_MAX`] in recent, which always has a hard max.
    pub max_squares: usize,
    /// How many of the reader's own turns stay on the strip even when the mode
    /// does not place them — the pinned band. `0` turns the band off.
    pub user_keep: usize,
}

/// The most squares a recent block shows when the caller leaves `max_squares`
/// at its default `0`. A block taller than the pane is a list whose end the
/// reader cannot reach, so recent mode always has a hard max.
pub const DEFAULT_RECENT_MAX: usize = 20;

/// The measured defaults, spelled once.
pub const STRIP_DEFAULTS: Options = Options {
    mode: Mode::Relative,
    // The intersection gradient runs from this floor to full size: a turn 40%
    // visible draws at 40%, and anything less than that stays at 40% rather than
    // shrinking out of reach. A floor near 1 compresses the gradient into a
    // range the reader cannot see, which is what 0.7 did.
    scale_min: 0.4,
    min_gap: 1.0,
    max_squares: 0,
    user_keep: 4,
};

impl Default for Options {
    fn default() -> Self {
        STRIP_DEFAULTS
    }
}

/// One square: where it sits on the strip, how big it draws, and whether the
/// reader is looking at it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Square {
    pub id: String,
    pub kind: TurnKind,
    /// In the mode's own space. Relative: the turn's first row *inside the
    /// window*, where `0.0` is the window's first row and `height - 1.0` its
    /// last, so a caller drawing `y * cell_height` puts the square on the row
    /// its turn starts on and a scroll moves it. Recent: the turn's place in
    /// the block, `0.0` the oldest, so a caller steps down its own track and a
    /// scroll moves nothing.
    pub y: f64,
    /// How big the turn draws, as the part of it the reader can see: relative
    /// mode sizes the square from the placement's own intersection with the
    /// window, floored at `options.scale_min` — a turn scrolled halfway out is
    /// half a square, a turn wholly in view is full size, and a sliver keeps the
    /// floor. Always `1.0` in recent mode, which draws every square the same.
    pub scale: f64,
    pub active: bool,
}

/// The strip in [`Mode::Recent`]: the newest turns of the session, one square
/// each, uniform, oldest first. `y` counts places in the block — no row, no
/// span — so the caller centres the block on its own track and a scroll moves
/// nothing. `rows` is the reader's window height: the track the block is
/// centred in. There is no band: a list of the newest turns already holds the
/// reader's own prompts, so a band beside it would say a turn twice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentStrip {
    pub squares: Vec<Square>,
    pub rows: f64,
}

/// The strip in [`Mode::Relative`]: the window's turns, each at the row it
/// starts on, plus the pinned band.
///
/// `squares[..band]` are the pinned ones, oldest first, `y` counting positions
/// in the band rather than rows — they are the reader's own turns kept on the
/// strip while the mode places none of them, so they are not positions and the
/// caller draws them in their own lane. Everything after them is a window row:
/// `y` in `0..rows`, `y * cell_height` from the window's first row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelativeStrip {
    pub squares: Vec<Square>,
    pub band: usize,
    /// The window's height in rows: what a relative square's `y` is measured in.
    pub rows: f64,
}

/// The strip, in whichever mode was asked for. Tagged on the wire (`"mode"`),
/// so a client branches once and never has to guess which space `y` is in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Layout {
    Relative(RelativeStrip),
    Recent(RecentStrip),
}

impl Layout {
    /// The strip's squares, oldest first: the band's own first in relative mode.
    pub fn squares(&self) -> &[Square] {
        match self {
            Layout::Relative(strip) => &strip.squares,
            Layout::Recent(strip) => &strip.squares,
        }
    }

    /// How many of [`Layout::squares`] are the pinned band, at its head.
    /// Relative mode only: a recent strip has no band.
    pub fn band(&self) -> usize {
        match self {
            Layout::Relative(strip) => strip.band,
            Layout::Recent(_) => 0,
        }
    }
}
