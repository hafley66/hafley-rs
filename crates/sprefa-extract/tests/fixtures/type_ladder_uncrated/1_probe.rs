use std::process::Output;

pub fn probe() -> Output {
    std::process::Command::new("true").output().unwrap()
}
