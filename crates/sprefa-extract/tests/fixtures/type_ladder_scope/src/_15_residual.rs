pub struct ResidualTarget;
pub struct Residual;

impl Residual {
    pub fn explicit(self: Box<Self>) {}
}

pub trait AliasSlot {
    type Item;
}

impl AliasSlot for Residual {
    type Item = ResidualTarget;
}

pub struct NestedGeneric<T = ResidualTarget>(pub T)
where
    Option<ResidualTarget>: Clone;
