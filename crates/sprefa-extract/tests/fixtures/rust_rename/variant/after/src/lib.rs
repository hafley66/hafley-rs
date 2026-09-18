mod uses;
mod other;

pub enum Kind {
    Prior,
    New,
}

pub fn make() -> Kind {
    Kind::Prior
}

pub fn label(k: &Kind) -> &'static str {
    match k {
        Kind::Prior => "old",
        Kind::New => "new",
    }
}
