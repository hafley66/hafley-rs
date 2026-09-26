pub struct Output;

mod tests {
    use std::process::Output;

    pub fn nested() -> Output {
        std::process::Command::new("true").output().unwrap()
    }
}
