use crate::_0_types::Device;

trait Ping {
    fn ping(&self);
}

impl Ping for Device {
    fn ping(&self) {
        self.ping();
    }
}
