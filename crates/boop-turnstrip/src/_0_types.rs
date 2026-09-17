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
}

/// Pinned by measurement on a busy pane, not by taste: `square_height` and
/// `strip_max` mirror the CSS, the rest are the TypeScript `STRIP_DEFAULTS`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub square_height: f64,
    pub strip_max: f64,
    /// Scale given to a turn twice the window's median: `1 + flex` at
    /// `2 × L_ref`.
    pub ratio_flex: f64,
    pub scale_min: f64,
    pub scale_max: f64,
    pub block_min: f64,
    /// The strip's own cap: [`crate::layout`] keeps only the newest
    /// `max_squares` rows of the window, `0` meaning no cap. This is a
    /// rendering budget the *server* owns, not a measurement: because the cap
    /// trims the window before anything is measured, it changes `span` and with
    /// it every square's `y`, so the same pane at two caps is two different
    /// strips and a caller must not mix their output.
    pub max_squares: usize,
}

/// The measured defaults, spelled once.
pub const STRIP_DEFAULTS: Options = Options {
    square_height: 9.0,
    strip_max: 320.0,
    ratio_flex: 0.35,
    scale_min: 0.7,
    scale_max: 1.9,
    block_min: 6.0,
    max_squares: 24,
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
    pub y: f64,
    pub scale: f64,
    pub active: bool,
}

/// The on-screen range, in the same `0..track` space the squares use.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub top: f64,
    pub height: f64,
}

/// The strip. `row_at` in the TypeScript is a closure over the placements;
/// here the same mapping is `_3_layout::row_at`, called directly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub squares: Vec<Square>,
    /// Estimated rows the whole window occupies, the strip's denominator.
    pub span: f64,
    pub block: Block,
}
