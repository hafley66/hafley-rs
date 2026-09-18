mod uses;
mod other;

pub enum Kind {
    Old,
    New,
}

pub fn make() -> Kind {
    Kind::Old
}

pub fn label(k: &Kind) -> &'static str {
    match k {
        Kind::Old => "old",
        Kind::New => "new",
    }
}
