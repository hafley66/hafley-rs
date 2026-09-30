pub struct Context {
    pub id: u32,
}

pub fn current() -> Context {
    Context { id: 0 }
}
