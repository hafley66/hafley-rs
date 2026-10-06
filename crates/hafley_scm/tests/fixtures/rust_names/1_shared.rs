pub fn target() {}
pub struct Item;
impl Item {
    pub fn method(&self) {}
}

impl Item {
    pub fn new() -> Self {
        Self::helper();
        Item
    }
    fn helper() {}
}
pub type Alias = Item;
pub enum Kind {
    Variant(u32),
}

pub trait Factory {
    fn make() -> Self;
}
impl Factory for Item {
    fn make() -> Self {
        Item
    }
}

impl Kind {
    pub fn create() -> Self {
        Self::Variant(1)
    }
}
