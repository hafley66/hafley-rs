//! Neutral mechanics contract for shared, character-independent fighter
//! transitions.
//!
//! Generated functions never read game memory. Callers supply the fighter
//! input, the unresolved tuning limits and the character attributes; the
//! authored evaluator in `1_evaluate.rs` interprets the generated rule data and
//! returns the source decision or the ordered effect sequence. Provenance for
//! every rule (source symbol, line span, call list and source expression text)
//! travels with the generated data in [`RuleProvenance`].

/// Fighter input the shared transition rules read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FighterInput {
    /// Signed horizontal stick position for the current frame.
    pub stick_x: f32,
    /// Fighter facing direction, `+1.0` right and `-1.0` left.
    pub facing: f32,
}

/// Tuning limits with no retained value. They stay typed inputs so the
/// generated port never invents a number; the caller supplies the game value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommonTuning {
    /// Upper bound on `stick_x * facing` for a turn request.
    pub turn_threshold: f32,
    /// Backward-stick magnitude used as the signed forward/backward jump split.
    pub jump_back_threshold: f32,
}

/// Character attributes the shared aerial-jump rule reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FighterAttrs {
    /// Horizontal air-jump velocity scale.
    pub air_jump_h_scale: f32,
    /// Initial jump velocity.
    pub jump_initial_speed: f32,
    /// Vertical air-jump velocity scale.
    pub air_jump_v_scale: f32,
}

/// A vector computed from typed attributes, never read from game memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Motion selected by a shared transition rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    JumpForward,
    JumpBackward,
    AirJumpForward,
    AirJumpBackward,
}

/// Motion flag passed to a motion change. The extracted subset only observes
/// the source `none` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionFlags {
    None,
}

/// One ordered effect from a shared transition rule. Stack-only: `Copy`, no heap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MechanicEffect {
    /// Per-callback common prelude with no modeled arguments.
    BeginJump,
    /// Select a motion with its animation timing.
    EnterMotion {
        motion: Motion,
        flags: MotionFlags,
        anim_start: f32,
        anim_speed: f32,
        anim_blend: f32,
    },
    /// Set a boolean jump parameter and a scalar scale.
    SetJumpParams { enabled: bool, scale: f32 },
    /// Write an integer command value.
    SetCommandValue { slot: u32, value: u32 },
    /// Write a boolean fighter flag.
    SetFlag { value: bool },
    /// Select an aerial motion and launch with a computed velocity.
    LaunchJump {
        motion: Motion,
        velocity: Vec3,
        flag: bool,
    },
}

/// Identity of a generated rule. Neutral names come from the source events
/// (`turn_request`, `takeoff`, `air_jump`), not from source callback symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleId {
    TurnRequest,
    Takeoff,
    AirJump,
}

/// Comparison operators the extracted subset carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

/// A fighter input field read by a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputField {
    StickX,
    Facing,
}

/// A tuning limit read by a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuningField {
    TurnThreshold,
    JumpBackThreshold,
}

/// A character attribute read by a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrField {
    AirJumpHScale,
    JumpInitialSpeed,
    AirJumpVScale,
}

/// A local value bound earlier in a rule body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalSlot {
    Motion,
    Velocity,
}

/// A pure expression over typed rule inputs. Generated as `&'static Expr` data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Expr {
    Input(InputField),
    Tuning(TuningField),
    Attr(AttrField),
    Local(LocalSlot),
    Motion(Motion),
    Flag(bool),
    Number(f32),
    Count(u32),
    Neg(&'static Expr),
    Mul(&'static Expr, &'static Expr),
    Compare(&'static Expr, CompareOp, &'static Expr),
    Select {
        condition: &'static Expr,
        yes: &'static Expr,
        no: &'static Expr,
    },
    Vector {
        x: &'static Expr,
        y: &'static Expr,
        z: &'static Expr,
    },
}

/// An effect template. Dynamic arguments stay expressions the evaluator binds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    BeginJump,
    EnterMotion {
        motion: &'static Expr,
        flags: MotionFlags,
        anim_start: f32,
        anim_speed: f32,
        anim_blend: f32,
    },
    SetJumpParams {
        enabled: &'static Expr,
        scale: f32,
    },
    SetCommandValue {
        slot: u32,
        value: &'static Expr,
    },
    SetFlag {
        value: &'static Expr,
    },
    LaunchJump {
        motion: &'static Expr,
        velocity: LocalSlot,
        flag: &'static Expr,
    },
}

/// One ordered statement in a rule body: a pure local binding or one effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Statement {
    Bind { slot: LocalSlot, value: &'static Expr },
    Emit(&'static Effect),
}

/// Source identity retained beside the neutral rule data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleProvenance {
    /// Original decomp callback symbol.
    pub source_symbol: &'static str,
    /// Inclusive source function line extent.
    pub function_lines: (u32, u32),
    /// 1-based `(line, column, end_line, end_column)` extent of the lowered node.
    pub span: (u32, u32, u32, u32),
    /// Direct source calls retained for the callback.
    pub calls: &'static [&'static str],
    /// Source guard expression text, empty when the rule lowers a callback body.
    pub source_guard: &'static str,
    /// Source local-binding expression text, in source order.
    pub source_bindings: &'static [&'static str],
}

/// One generated neutral rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub id: RuleId,
    /// Decision predicate, or `None` when the rule lowers a callback body.
    pub guard: Option<&'static Expr>,
    pub statements: &'static [Statement],
    pub provenance: RuleProvenance,
}
