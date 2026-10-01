//! Café — the non-ASCII span fixture: é and — precede every name on its line.
pub mod words;

/* é — */ pub fn greet() -> u32 {
    let label = "naïve — é"; let n = words::count_words(label);
    /* é */ n + words::count_words("é—é") + words::twice("—")
}
