pub struct One;
impl One { pub fn hit(&self) {} }
pub struct Two;
impl Two { pub fn hit(&self) {} }
pub fn caller(x: One) { x.hit(); }
