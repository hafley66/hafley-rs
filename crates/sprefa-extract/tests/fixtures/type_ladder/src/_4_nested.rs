use crate::_0_types::{A, B, C, V, W};

pub struct Nest {
    pub depth_1: Option<A>,
    pub depth_2: Vec<Option<B>>,
    pub depth_many: Result<Vec<A>, Box<Option<C>>>,
}

impl V for Nest {
    type Out = A;
}

impl W for Nest {
    type Out<Y> = Vec<Y>;
}

pub fn assoc_bound<X: V<Out = B>>(_x: X) {}

pub fn projection<X: V>(_x: X, _out: <X as V>::Out) {}

pub fn gat_use<X: W>(_x: X, _out: X::Out<C>) {}
