use crate::_0_types::{free_zero, Device, Action};

pub fn nested_calls() {
    fn inner(device: &Device) { device.act(); }
    let device = Device::make();
    inner(&device);
    (|| free_zero())();
}
