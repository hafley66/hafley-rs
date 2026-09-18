mod util;

struct Helper;

fn a() {
    use crate::util::Helper;
    Helper::new();
}

fn b() -> Helper {
    Helper
}
