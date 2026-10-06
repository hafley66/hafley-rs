// one
// two
#[cfg(test)]
// between
fn helper() { assert!(true) }

#[cfg(test)]
#[inline]
fn bare() { 1; }
