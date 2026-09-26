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

pub fn keeps_using() -> u32 {
    use crate::_0_base::Base as Local;
    let _local = Local;
    Plain.show()
}
