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

/// Shell art slot (`Fighter::char_id`) -> distinct-kit row in `roster()`. LOCKED to the shell art
/// order (`rust-sim/shell/src/roster.rs` `roster()` = `[frog, zombie]` + `assets/roster.json`
/// `[falcon, lucario, ness, lucas, kermit, obama]`), because `char_id` is written by char-select
/// against THAT list. Each slot resolves the physics of the fighter whose ART sits there; a slot
/// with no bespoke kit yet borrows the closest existing one (kneeman = baseline, falcon = fast,
/// lucas = floaty) until its own `CharSpec` lands.
///
/// | char_id | shell art | kit row | note |
/// |---------|-----------|---------|------|
/// | 0 | frog     | 0 kneeman | P1 default placeholder = baseline (also the flat/panel row) |
/// | 1 | zombie   | 1 falcon  | P2 default placeholder |
/// | 2 | falcon   | 1 falcon  | the actual Falcon |
/// | 3 | lucario  | 1 falcon  | fast rushdown -> Falcon kit for now |
/// | 4 | ness     | 2 lucas   | floaty PK sibling -> Lucas physics for now |
/// | 5 | lucas    | 2 lucas   | the actual Lucas (fast-Lucas pass) |
/// | 6 | kermit   | 0 kneeman | meme placeholder = baseline |
/// | 7 | obama    | 0 kneeman | meme placeholder = baseline |
///
/// An out-of-range slot maps to row 0 (baseline), never a floaty outlier -- that clamp is what the
/// pre-alignment roster got wrong: it landed on Lucas, so every shell pick past index 2 inherited
/// his low gravity.
pub const ART_SLOT_ROW: [u8; 8] = [0, 1, 1, 1, 2, 2, 0, 0];

/// Map a shell art slot (`char_id`) onto its distinct-kit `roster()` row. Out-of-range -> row 0.
pub fn art_slot_row(char_id: u8) -> usize {
    ART_SLOT_ROW.get(char_id as usize).copied().unwrap_or(0) as usize
}
