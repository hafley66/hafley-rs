//! Words — é.

/* Counts — café words. */ pub fn count_words(text: &str) -> u32 {
    let _dash = "— é"; text.len() as u32
}

/* — */ pub fn twice(text: &str) -> u32 {
    let _accent = "é — é"; count_words(text) + count_words("—")
}
