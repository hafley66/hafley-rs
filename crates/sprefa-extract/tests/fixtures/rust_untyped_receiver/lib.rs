pub struct Defs {
    v: Vec<u32>,
}

impl Defs {
    pub fn push(&mut self, x: u32) {
        self.v.push(x)
    }
}
