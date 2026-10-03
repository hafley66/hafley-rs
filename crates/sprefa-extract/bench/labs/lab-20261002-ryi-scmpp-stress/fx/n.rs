mod m {
    fn outer(a: u8) -> u8 {
        let k = |x: u8| { inner(x) + helper(a) };
        if a > 0 { inner(a) } else { k(a) }
    }
    fn inner(v: u8) -> u8 { v }
}
fn helper(a: u8) -> u8 { a }
