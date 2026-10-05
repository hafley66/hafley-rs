use crate::helpers::a;
use crate::helpers::b;
use crate::helpers::c;
use crate::helpers::keep;

pub fn lifted() { a(); b(); c(); keep(); }
pub fn stayed() { keep(); }
