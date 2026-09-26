pub fn default_probe() {
    let _ = crate::_0_left::Defaults {
        value: 1,
        ..Default::default()
    };
}

pub fn typed_default_probe() {
    let _: crate::_0_left::Defaults = Default::default();
}
