# Shared game crate destination

All first-party reusable game crates are to live directly under this directory.
Do not nest crates under `../smash/`. Existing `../shared/` and lab packages are
migration sources; they have not moved yet. Preserve upstream submodule layouts.

Migration A1 must move source/tests/provenance and update every consumer together.
Choose Cargo workspace membership explicitly, retain dependency pins, and verify
the existing lab and app consumers. No empty substitute libraries or duplicate
cores. Non-crate workflow/assets/evidence can remain in their current directories.
