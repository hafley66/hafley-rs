use crate::_0_base::{
    Base,
    Show,
};

/// Documented item.
/// Second doc line.
#[derive(Debug, Clone)]
pub struct Documented {
    pub base: Base,
}

pub struct Plain;

impl Show for Plain {
    fn show(&self) -> u32 {
        1
    }
}

