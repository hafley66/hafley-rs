pub fn target(value: i32) -> i32 {
    helper(value)
}

pub fn caller(value: i32) -> i32 {
    target(value)
}

fn helper(value: i32) -> i32 {
    value + 1
}
