fn greet(g: &str) -> String { String::new() }
fn echo(x: &str) -> String { String::new() }
fn use_up(v: &str) {}
fn f(name: &str) {
    let s = greet(name);
    let u = echo(&s);
    use_up(&u);
}
