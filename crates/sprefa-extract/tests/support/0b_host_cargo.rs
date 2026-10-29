//! rcargo builds this binary on another host and runs it here: compile-time `CARGO`,
//! `CARGO_HOME` and `RUSTUP_HOME` name that host's paths. Spawns use this host's.

/// The test binary was compiled under this machine's home.
pub fn built_here() -> bool {
    std::env::var("HOME").is_ok_and(|home| home == env!("HOME"))
}

/// `cargo` on this host: the runtime `CARGO`, else `$HOME/.cargo/bin/cargo`.
pub fn cargo() -> std::process::Command {
    let cargo = std::env::var_os("CARGO")
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".cargo/bin/cargo").into()))
        .unwrap_or_else(|| env!("CARGO").into());
    let mut command = std::process::Command::new(cargo);
    if !built_here() {
        command.env("RCARGO_OFF", "1").env_remove("CARGO_HOME").env_remove("RUSTUP_HOME").env_remove("CARGO_TARGET_DIR");
    }
    command
}
