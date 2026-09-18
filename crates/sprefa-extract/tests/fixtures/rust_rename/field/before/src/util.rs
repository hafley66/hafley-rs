pub struct Helper {
    pub size: u32,
}

impl Helper {
    pub fn grow(&mut self) {
        self.size += 1;
    }
}
