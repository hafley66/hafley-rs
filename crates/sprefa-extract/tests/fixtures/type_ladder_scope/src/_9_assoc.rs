pub trait Maker {
    type Item;
}

pub struct Holder;

impl Maker for Holder {
    type Item = crate::_0_alias::LocalThing;
}
