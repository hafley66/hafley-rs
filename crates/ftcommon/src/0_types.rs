//! Typed query/effect boundary for the generated ftCommon port.
//!
//! Generated functions never read game memory. Callers supply the fighter query
//! and the unresolved common-data fields; each function returns the source
//! decision. Engine calls outside the translated subset stay documented in the
//! generated file rather than being emulated here.

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

/// Motion ids the translated callbacks select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FtMotionId {
    JumpF,
    JumpB,
    JumpAerialF,
    JumpAerialB,
}
