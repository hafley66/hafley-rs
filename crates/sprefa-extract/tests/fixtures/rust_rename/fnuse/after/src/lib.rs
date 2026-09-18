mod util;

struct Helper;

fn a() {
    use crate::util::Tool;
    Tool::new();
}

fn b() -> Helper {
    Helper
}
