//! Stroke materials: GateSide, StrokeProps, and the named-preset StrokeRegistry.
//! Split out of stage/mod.rs (R5 ratchet) -- re-exported there, call sites unchanged.

use serde::{Deserialize, Serialize};

/// One-way pass orientation (plans/body-unify.md step 5): `Off` is plain solid/soft; the two
/// `Pass*` variants flip which side of `contact::segment_gate_normal` admits a crossing.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum GateSide {
    #[default]
    Off,
    PassForward,
    PassBackward,
}

/// Per-stroke material. Plain `Copy` data stamped onto every node a tool lays, so "different pens →
/// different surfaces" needs no new state shape — just a different `StrokeProps`. Lives on the path
/// (and is editable per-tool via `DrawTool::props`).
// parity(v1-ink-material-registry): each stroke carries authored lifetime, classification, restitution, density, solidity, zone, valve, gravity, and spin material values
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StrokeProps {
    pub stroke_life: i64, // frames the whole stroke survives after it's FINISHED, then it exits at once
    pub floor_tol: f32,   // |slope angle| ≤ this ⇒ Floor (radians)
    pub wall_tol: f32,    // |slope angle| ≥ this ⇒ Wall (radians)
    pub ledge_curve: f32, // Δangle between adjacent segments at a Floor tip ≥ this ⇒ grabbable Ledge
    pub min_seg: f32,     // segments shorter than this (px) classify as None
    pub bounce: f32,      // wall restitution if `solid`
    pub density: f32, // mass per px of stroke length (finalize: mass = Σ|seg| · density); 0 = never a body
    pub solid: bool, // true = blocks all sides; false = soft (land from above, drop through w/ down)
    pub force_wall: bool, // classify EVERY segment as Wall (ignore slope) — a pure wall pen, no hollow bits
    pub zone: bool,       // still ink of this material EXTENDS the blast zone (the zone-maker pen)
    pub gate_side: GateSide, // Off = unchanged solid/soft
    // appended last (bincode positional): a per-stroke gravity multiplier, not a per-body special
    // case. `integrate_ink` always multiplies by this row (plans/body-unify.md step 6) -- 1.0
    // (every existing preset) reproduces today's fall exactly; 0.0 (the ship hull's row) means
    // gravity contributes NOTHING, so thrust is the only force. The row IS the mechanism: no
    // owner/slot check anywhere in the integrator.
    pub gravity_scale: f32,
    // appended last (bincode positional, after gravity_scale): per-frame spin retention.
    // `integrate_ink` multiplies `omega` by this row every frame BEFORE the `rot += omega`
    // accrual -- 1.0 (every ordinary preset) keeps the strike/billiard tumble exactly as
    // before; 0.0 (the ship hull's row) means an off-center hit never turns the body, so the
    // hatch and station anchors stay upright. Same row-not-special-case rule as gravity_scale.
    pub spin_scale: f32,
}

impl StrokeProps {
    /// Baseline pen. Classifies by slope — flat is Floor (stand on it), near-vertical is Wall
    /// (blocks), the middle band is a sloped Floor (stand/slide, never a hole). Blue (Floor/Ledge)
    /// segments are SOFT: land from above, tap down to drop through, never blocked from below.
    /// Purple (Wall) segments block regardless — walls ignore `solid`. The ship hull inherits
    /// this, so dropping OUT through the cockpit bowl's bottom is intended. No segment is left as
    /// an empty None surface, so the whole stroke is a real collision face.
    pub const PEN: Self = Self {
        // never expires (building material). The decay loop + shell countdown tag both honor the
        // < 0 sentinel, so the timer machinery stays intact — set a positive frame count to get
        // timed ink back (panel: stroke life slider).
        stroke_life: -1,
        floor_tol: 0.55,  // ~31° — flat enough to just stand
        wall_tol: 1.20,   // ~69° — steep enough to be a blocking wall
        ledge_curve: 0.7, // ~40° corner makes a lip grabbable
        min_seg: 10.0,
        bounce: 0.4,
        density: 1.0, // 1 mass unit per px: a 300px stroke weighs 300 (kb formula rescales)
        solid: false, // blue floors are soft platforms: land on top, held down drops through
        force_wall: false, // classify by slope (flat Floor / steep Wall / mid sloped Floor)
        zone: false,  // the prototype pencil: temporary ink, does NOT grow the blast zone
        gate_side: GateSide::Off, // ordinary solid/soft, not a one-way gate
        gravity_scale: 1.0, // full gravity: every ordinary stroke falls exactly as before
        spin_scale: 1.0, // full tumble: off-center hits spin the body exactly as before
    };

    /// Permanent piece material (the tetris gun): never times out (`stroke_life < 0` is the
    /// never-expires sentinel the decay loop honors), dies only past the blast zone. SOFT like a
    /// soft platform — stand on top, drop through with held down. `ledge_curve` sits above π
    /// (`ang_diff` maxes at π) so the piece's right-angle corners are plain Floor, never grabbable
    /// lips: the whole top reads blue, not a ring of yellow.
    pub const TETRIS: Self = Self {
        stroke_life: -1,
        solid: false,
        ledge_curve: 9.0,
        ..Self::PEN
    };
}

/// Index into a `StrokeRegistry`'s preset table. Plain `u8` so it rides inside `Item`/`SimState` as
/// Copy data (no trait object: replay stores what HAPPENED, so the material must be resolvable from
/// data alone, not a boxed algorithm). Row 0 is always the default.
pub type StrokeId = u8;

/// How many named stroke presets the registry holds. Tune isn't in the per-frame rollback checksum
/// (it's constant config), so this can be decently wide without touching sync cost.
pub const STROKE_SLOTS: usize = 16;

/// The named-preset table of stroke materials — the "registry" a `StrokeId` resolves against. Think
/// CSS: `StrokeProps` is the property bag, this is the stylesheet, row 0 is the cascade root/default.
/// Owned by `Tune` (panel-editable), Copy + serde so it round-trips with the rest of config.
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeRegistry {
    pub presets: [StrokeProps; STROKE_SLOTS],
}

impl StrokeRegistry {
    /// Registry row of the permanent tetris-platform material.
    pub const TETRIS_ROW: StrokeId = 1;

    /// Registry row of the zone-maker material (`StrokeProps.zone = true`): still ink of this
    /// material EXTENDS the live blast zone under `ZoneMode::InkExtends` (see `ink_blast_zone`).
    /// Otherwise plain PEN (permanent, soft) -- only the zone bit differs.
    pub const ZONE_ROW: StrokeId = 2;

    /// Registry row of the one-way gate material (`gate_side = PassForward`): every segment
    /// admits a crossing WITH its drawing-order gate normal (`contact::segment_gate_normal`,
    /// the stroke's own a->b tangent rotated 90 degrees) and blocks against it -- so the side
    /// the pen was traveling toward is the pass side. The shell draws pass-side ticks on gated
    /// segments, so which way a drawn gate admits is visible, not guessed.
    pub const GATE_ROW: StrokeId = 3;

    /// Every slot starts at the baseline pen; row 0 is the default, row 1 the permanent tetris
    /// material, row 2 the zone-maker pen, row 3 the one-way gate pen. Panels/serde override
    /// rows later.
    pub const DEFAULT: Self = {
        let mut presets = [StrokeProps::PEN; STROKE_SLOTS];
        presets[Self::TETRIS_ROW as usize] = StrokeProps::TETRIS;
        presets[Self::ZONE_ROW as usize] = StrokeProps {
            zone: true,
            ..StrokeProps::PEN
        };
        presets[Self::GATE_ROW as usize] = StrokeProps {
            gate_side: GateSide::PassForward,
            ..StrokeProps::PEN
        };
        Self { presets }
    };

    /// Resolve a `StrokeId` to its material, falling back to the default row (0) on an out-of-range id.
    pub fn get(&self, id: StrokeId) -> StrokeProps {
        *self.presets.get(id as usize).unwrap_or(&self.presets[0])
    }

    /// The default stroke material (row 0) — the cascade root every unstyled path inherits.
    pub fn default_props(&self) -> StrokeProps {
        self.presets[0]
    }
}
