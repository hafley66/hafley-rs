//! The one name a harness has: `SessionRef`, `Route` and every `--harness`
//! argument carry this enum. `dict_harness` stays text on the way out: a store
//! written by an older binary holds values this enum never named (`gemini`
//! from the acpx preset), so no SQL read maps that column into `HarnessId`;
//! readers keep `String` and call `HarnessId::parse` where a variant matters.

use std::fmt;
use std::str::FromStr;

use anyhow::{anyhow, Error, Result};

/// One agent harness boop can read, spawn and address. Declaration order is
/// alphabetical by `as_str`, so the derived `Ord` is the order a registry lists.
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum HarnessId {
    Claude,
    Codex,
    Kimi,
    Opencode,
}

impl HarnessId {
    /// Every harness, in `as_str` order.
    pub const ALL: [HarnessId; 4] = [
        HarnessId::Claude,
        HarnessId::Codex,
        HarnessId::Kimi,
        HarnessId::Opencode,
    ];

    /// The `--harness` value, the `dict_harness` value, and a route's `harness`.
    pub const fn as_str(self) -> &'static str {
        match self {
            HarnessId::Claude => "claude",
            HarnessId::Codex => "codex",
            HarnessId::Kimi => "kimi",
            HarnessId::Opencode => "opencode",
        }
    }

    /// The harness a short id names, case folded, or `None` when it names none.
    pub fn parse(value: &str) -> Option<HarnessId> {
        let value = value.trim().to_ascii_lowercase();
        HarnessId::ALL
            .into_iter()
            .find(|harness| harness.as_str() == value)
    }


}

impl fmt::Display for HarnessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for HarnessId {
    type Err = Error;

    fn from_str(value: &str) -> Result<HarnessId> {
        HarnessId::parse(value).ok_or_else(|| {
            anyhow!(
                "unknown harness `{value}`; registered harnesses: {}",
                HarnessId::ALL
                    .into_iter()
                    .map(HarnessId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HarnessId;

    /// RECEIPT. The `dict_harness` value and the `--harness` argument are the
    /// same text in both directions, so a stored row re-reads as its variant.
    #[test]
    fn every_variant_round_trips_through_its_short_id() {
        for harness in HarnessId::ALL {
            assert_eq!(HarnessId::parse(harness.as_str()), Some(harness));
            assert_eq!(harness.to_string(), harness.as_str());
            assert_eq!(harness.as_str().parse::<HarnessId>().unwrap(), harness);
        }
        assert_eq!(HarnessId::parse("  Codex "), Some(HarnessId::Codex));
        assert_eq!(HarnessId::parse("gemini"), None);
    }

    /// RECEIPT. Serde is the registry.json spelling; a route round-trips.
    #[test]
    fn serde_is_the_lowercase_short_id() {
        let json = serde_json::to_string(&HarnessId::Opencode).unwrap();
        assert_eq!(json, "\"opencode\"");
        assert_eq!(
            serde_json::from_str::<HarnessId>("\"claude\"").unwrap(),
            HarnessId::Claude
        );
    }
}
