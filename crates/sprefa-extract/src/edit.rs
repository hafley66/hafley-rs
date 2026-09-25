use crate::lang::kotlin::KotlinSource;
use crate::lang::prolog::PrologSource;
use crate::lang::rust::RustSource;
use crate::lang::ts::TsSource;
use crate::edit_seams::RehomeArm;
use crate::edit_seams::Rename;
use crate::lang::source_for;
use crate::edit_seams::Cleave;

/// The `Rehome` roster: one impl per language `extract move` can rehome, in
/// `sources()` order. A language with no impl here is a named stop, never a
/// `match` arm in the move core.
pub fn rehomes() -> &'static [RehomeArm] {
    const ROSTER: [RehomeArm; 4] = [
        RehomeArm {
            core: &RustSource,
            manifests: Some(&RustSource),
            shim: None,
            text_spellings: None,
            plan_check: Some(&RustSource),
        },
        RehomeArm {
            core: &KotlinSource,
            manifests: None,
            shim: None,
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &PrologSource,
            manifests: None,
            // Disabled: the shim leg rode the YAML rule engine.
            shim: None,
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &TsSource,
            manifests: Some(&TsSource),
            shim: None,
            text_spellings: Some(&TsSource),
            plan_check: Some(&TsSource),
        },
    ];
    &ROSTER
}

/// The `Rename` roster, in `sources()` order. Membership is "has a scope plane
/// with exact identifier spans", a different question from `rehomes()`'s.
pub fn renames() -> &'static [&'static dyn Rename] {
    &[&TsSource, &RustSource, &KotlinSource, &PrologSource]
}

/// The `Rehome` that owns `path`, under the SAME first-match law `sources()`
/// states: `"x.kts".ends_with(".ts")` is true, so `TsSource` matches a kotlin
/// script too and only `source_for`'s own winner may claim it.
pub fn rehome_for(path: &str) -> Option<&'static RehomeArm> {
    let owner = source_for(path)?.name();
    rehomes().iter().find(|arm| arm.name() == owner)
}

/// The `Rename` that owns `path`, under the SAME first-match law `rehome_for`
/// states: only `source_for`'s own winner may claim a path.
pub fn rename_for(path: &str) -> Option<&'static dyn Rename> {
    let owner = source_for(path)?.name();
    renames().iter().copied().find(|arm| arm.name() == owner)
}

/// The `Cleave` roster, in `sources()` order. Membership is "this language can
/// be text-edited by a verb", a third question again from `rehomes()`'s.
pub fn cleaves() -> &'static [&'static dyn Cleave] {
    &[&RustSource, &TsSource]
}

/// The `Cleave` that owns `path`, under the SAME first-match law `rehome_for`
/// states: only `source_for`'s own winner may claim a path.
pub fn cleave_for(path: &str) -> Option<&'static dyn Cleave> {
    let owner = source_for(path)?.name();
    cleaves().iter().copied().find(|arm| arm.name() == owner)
}
