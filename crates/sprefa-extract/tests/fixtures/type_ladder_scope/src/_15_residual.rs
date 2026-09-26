pub struct ResidualTarget;
pub struct Residual;

use crate::_0_alias::Shared;

impl crate::_0_alias::SharedTrait for Shared {
    fn cross_module(self: Box<Self>) {}
}

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
