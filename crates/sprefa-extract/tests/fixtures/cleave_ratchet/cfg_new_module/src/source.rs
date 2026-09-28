#[cfg_attr(
    feature = "checker",
    cfg(feature = "checker")
)]
#[cfg(feature = "checker")]
pub fn target() -> u8 { 1 }

#[cfg(feature = "checker")]
pub fn caller() -> u8 { target() }
