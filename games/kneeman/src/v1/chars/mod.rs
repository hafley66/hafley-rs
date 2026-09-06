//! Per-character `CharSpec` rows (plans/mod-api.md Tier 1: "character-as-file"; mirrors
//! `items/`'s "one file per kind" shape). `tune.rs` keeps the `CharSpec`/`Tune` TYPES and
//! the runtime resolve (`Tune::for_char`, `Tune::resolve`); this module owns each roster
//! row's DATA (`spec()`), plus the `roster()` builder `Tune::default` calls.
//!
//! Adding a character (plans/swordsman-lucas.md row 7): one new file here (a `spec()`
//! returning `CharSpec`) plus a row appended to `roster()` and a bump to `ROSTER_N`
//! (tune.rs) -- never an inline literal at the call site.

pub(crate) mod falcon;
pub(crate) mod kneeman;
pub(crate) mod lucas;

use crate::v1::{CharSpec, ROSTER_N};

/// The built-in roster: the DISTINCT physics kits, one row each. `char_id` does NOT index this
/// directly -- it is a shell art slot, mapped onto a row by `ART_SLOT_ROW` first (`Tune::for_char`).
/// Row order: 0 Knee Man (the flat/panel view), 1 Falcon, 2 Lucas. `Tune::default` reads this to
/// seed both the flat view (row 0) and the `roster` array.
pub fn roster() -> [CharSpec; ROSTER_N] {
    [kneeman::spec(), falcon::spec(), lucas::spec()]
}

/// Built-in art slots: frog, zombie, Falcon, Lucas. Imported art uses the baseline kit.
/// Keep this order aligned with the shell's roster; tests cover all u8 slots.
pub const ART_SLOT_ROW: [u8; 4] = [0, 1, 1, 2];

/// Map a shell art slot (`char_id`) onto its distinct-kit `roster()` row. Out-of-range -> row 0.
pub fn art_slot_row(char_id: u8) -> usize {
    ART_SLOT_ROW.get(char_id as usize).copied().unwrap_or(0) as usize
}
