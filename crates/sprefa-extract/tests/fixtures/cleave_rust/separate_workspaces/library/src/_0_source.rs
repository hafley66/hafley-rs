#[derive(Clone, Copy)]
pub struct Level(pub u32);

impl Level {
    pub fn value(self) -> u32 { self.0 }
}

#[derive(Clone, Copy)]
pub enum Signal { Ready(Level) }

impl Signal {
    pub fn value(self) -> u32 {
        match self { Self::Ready(level) => level.value() }
    }
}
