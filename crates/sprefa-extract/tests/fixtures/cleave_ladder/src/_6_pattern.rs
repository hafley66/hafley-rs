use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub struct Pattern(pub String);

impl Display for Pattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn compile(pattern: &Pattern) -> String {
    pattern.to_string()
}
