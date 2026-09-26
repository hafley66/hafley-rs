pub fn free_zero() {}
pub fn free_one(_: u8) {}
pub fn free_two(_: u8, _: u8) {}

pub struct Device;

impl Device {
    pub fn ping(&self) {}
    pub fn make() -> Self { Self }
}

pub trait Action {
    fn act(&self);
}

impl Action for Device {
    fn act(&self) {}
}
