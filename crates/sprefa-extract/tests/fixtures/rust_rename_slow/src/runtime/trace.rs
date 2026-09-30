pub struct Context {
    pub depth: u32,
}

pub fn root() -> Context {
    Context { depth: 0 }
}
