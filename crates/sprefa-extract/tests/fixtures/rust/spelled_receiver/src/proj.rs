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
    fn go(&self) -> u32;
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

pub struct Holder {
    pub w: Widget,
}
