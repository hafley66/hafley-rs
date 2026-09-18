pub struct CstProjector;

pub trait Project {
    fn project(&self) -> u32;
}

impl Project for CstProjector {
    fn project(&self) -> u32 {
        1
    }
}

pub trait Proj {
    fn run(&self) -> u32;
}

pub struct Widget {
    pub id: u32,
}

impl Widget {
    pub fn new() -> Self {
        Widget { id: 1 }
    }
    pub fn run(&self) -> u32 {
        self.id
    }
}

pub struct Box {
    pub inner: Widget,
}

pub fn spelled() -> u32 {
    CstProjector.project()
}

pub fn field_leg(b: &Box) -> u32 {
    b.inner.run()
}

pub fn ctor_leg() -> u32 {
    let w = Widget::new();
    w.run()
}

pub fn trait_bound_leg<P: Proj>(p: P) -> u32 {
    p.run()
}
