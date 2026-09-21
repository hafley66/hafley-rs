use crate::util::{load_config, slug};

pub fn boot(dir: &str) -> String {
    slug(&load_config(dir))
}
