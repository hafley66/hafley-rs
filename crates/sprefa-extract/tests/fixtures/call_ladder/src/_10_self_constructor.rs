pub struct First {
    pub value: u8,
}

impl First {
    pub fn first() -> Self {
        Self { value: 1 }
    }

    pub fn rows(&self) -> u8 {
        self.value
    }
}

pub fn first_rows() -> u8 {
    let first: First = First { value: 1 };
    first.rows()
}

pub struct Second {
    pub value: u8,
}

impl Second {
    pub fn second() -> Self {
        Self { value: 2 }
    }

    pub fn rows(&self) -> u8 {
        self.value
    }
}

pub fn second_rows() -> u8 {
    let second: Second = Second { value: 2 };
    second.rows()
}
