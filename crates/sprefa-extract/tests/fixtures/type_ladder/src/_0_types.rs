pub struct A;
pub struct B;
pub struct C;

pub trait T {}
pub trait U {}

pub trait V {
    type Out;
}

pub trait W {
    type Out<Y>;
}
