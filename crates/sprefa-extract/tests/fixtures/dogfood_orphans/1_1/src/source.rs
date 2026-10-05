use crate::helpers::{a, b};
use crate::helpers::{c, keep};

pub fn lifted() { a(); b(); c(); keep(); }
pub fn stayed() {}
