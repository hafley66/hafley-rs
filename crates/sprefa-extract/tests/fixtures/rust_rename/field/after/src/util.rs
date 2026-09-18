pub struct Helper {
    pub width: u32,
}

impl Helper {
    pub fn grow(&mut self) {
        self.width += 1;
    }
}
