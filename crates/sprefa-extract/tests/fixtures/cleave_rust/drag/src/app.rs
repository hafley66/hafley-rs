use crate::util::{label, load_config};

pub fn boot(dir: &str) -> String {
    label(&load_config(dir))
}
