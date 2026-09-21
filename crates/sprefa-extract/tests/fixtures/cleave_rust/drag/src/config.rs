use std::path::Path;

pub const CONFIG_NAME: &str = "config.json";

pub fn config_root() -> String {
    Path::new(".").display().to_string()
}
