use crate::_0_types::{free_zero, free_one, free_two, Device};

pub fn many_calls() {
    free_zero();
    free_one(1);
    free_two(1, 2);
    Device::make().ping();
}
