pub struct Box {
    pub inner: Widget,
}

pub fn field_leg(b: &Box) -> u32 {
    b.inner.run()
}

pub fn ctor_leg() -> u32 {
    let w = Widget::new();
    w.run()
}

pub fn trait_bound_leg<P: Proj>(p: P) -> u32 {
    p.go()
}
