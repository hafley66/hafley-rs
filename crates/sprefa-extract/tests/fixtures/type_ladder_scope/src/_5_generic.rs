pub struct Outer;
pub struct Generic<Outer>(std::marker::PhantomData<Outer>);
