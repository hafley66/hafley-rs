pub type Result<T> = std::result::Result<T, ()>;
pub struct Output;
pub struct LocalThing;
pub struct Shared;
pub trait SharedTrait {
    fn cross_module(self: Box<Self>);
}
