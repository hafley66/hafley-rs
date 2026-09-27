pub struct Counts(pub u32);

impl Counts {
    pub fn value(&self) -> u32 {
        self.0
    }
}
