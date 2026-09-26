pub struct Widget;

impl Widget {
    pub fn new() -> Self { Self }
}

pub struct Defaults { pub value: u8 }

impl Default for Defaults {
    fn default() -> Self { Self { value: 0 } }
}
