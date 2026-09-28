fn outer() {
    let x: u32 = 1;
    let x: String = x.to_string();
    let f = || x.len();
    let x: bool = true;
    let _ = (f(), x);
}

fn nested() {
    fn inner(value: u32) -> u32 { value + 1 }
    let _ = inner(1);
}

fn mutable() {
    let mut x = 0;
    let mut f = || { x += 1; };
    f();
}
