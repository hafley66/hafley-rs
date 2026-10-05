fn use_up(f: impl Fn() -> i32) {}
fn outer(value: i32) { use_up(|| value); }
