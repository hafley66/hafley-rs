use super::go_checker::{WireRow, WireStats};

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
pub struct WireFile {
    path: String,
    calls: Vec<WireRow>,
    types: Vec<WireRow>,
    /// `[relation, arg, ...]` per row; the ordinal is the wire's, minted here.
    #[serde(default)]
    tsi: Vec<Vec<serde_json::Value>>,
}

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WireLine {
    File(WireFile),
    Stats(WireStats),
}
