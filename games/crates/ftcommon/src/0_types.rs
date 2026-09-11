//! Typed query/effect boundary for the generated ftCommon port.
//!
//! Generated functions never read game memory. Callers supply the fighter query,
//! the unresolved common-data fields and the character attributes; each function
//! returns the source decision or the ordered effect sequence. Effect variants
//! keep the decomp symbol where the semantics are not modeled here.

/// Fighter inputs the translated callbacks read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FighterQuery {
    /// `fp->input.lstick[0].x`.
    pub lstick_x: f32,
    /// `fp->facing_dir`.
    pub facing_dir: f32,
}

/// `p_ftCommonData` fields with no retained value. They stay typed inputs so the
/// generated port never invents a number; the caller supplies the game value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommonData {
    /// `p_ftCommonData->x34`.
    pub x34: f32,
    /// `p_ftCommonData->x78`.
    pub x78: f32,
}

/// `fp->co_attrs` fields the translated callbacks read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoAttrs {
    /// `fp->co_attrs.air_jump_h_multiplier`.
    pub air_jump_h_multiplier: f32,
    /// `fp->co_attrs.jump_v_initial_velocity`.
    pub jump_v_initial_velocity: f32,
    /// `fp->co_attrs.air_jump_v_multiplier`.
    pub air_jump_v_multiplier: f32,
}

/// `Vec3` computed from typed attributes, never read from game memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Motion ids the translated callbacks select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FtMotionId {
    JumpF,
    JumpB,
    JumpAerialF,
    JumpAerialB,
}

/// `Ft_MF_*` motion flags the translated callbacks pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionFlags {
    /// `Ft_MF_None`.
    None,
}

/// One ordered effect from a translated callback. Stack-only: `Copy`, no heap.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FtCommonEffect {
    /// `ftCommon_8007D5D4(fp)`.
    ftCommon_8007D5D4,
    /// `Fighter_ChangeMotionState(gobj, msid, Ft_MF_None, 0.0F, 1.0F, 0.0F, NULL)`.
    Fighter_ChangeMotionState {
        motion: FtMotionId,
        flags: MotionFlags,
        anim_start: f32,
        anim_speed: f32,
        anim_blend: f32,
    },
    /// `ftCo_800CB110(gobj, arg1, jump_mul)`.
    FtCo_800CB110 { arg1: bool, jump_mul: f32 },
    /// `fp->x2227_b0 = value`.
    WriteX2227B0 { value: bool },
    /// `fp->cmd_vars[0] = value`.
    WriteCmdVars0 { value: u32 },
    /// `ftCo_800CBAC4(gobj, msid, &vel, arg3)`.
    FtCo_800CBAC4 {
        motion: FtMotionId,
        velocity: Vec3,
        arg3: bool,
    },
}
