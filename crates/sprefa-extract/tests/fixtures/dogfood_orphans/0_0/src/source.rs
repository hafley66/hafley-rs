use crate::helpers::{a, b, c, keep};

pub fn lifted() { a(); b(); c(); keep(); }
pub fn stayed() { keep(); }
