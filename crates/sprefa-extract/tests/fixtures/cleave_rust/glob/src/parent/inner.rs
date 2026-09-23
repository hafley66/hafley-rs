use super::*;

pub fn lifted(value: Kind) -> Kind {
    value
}

pub fn caller(value: Kind) -> Kind {
    lifted(value)
}
