# Source receipts

Consolidated 2026-09-06 in the hafley-rs repository, worktree `hafley-rs-game-runtime`,
branch `astra/game-runtime`. Other checkouts below are reference inputs, not build dependencies.

| Source checkout | Revision | Used for |
| --- | --- | --- |
| kneeman-lines/6_recovery | 8950de7e4c1c19ab9827b20ef21716a457ea9758 | Godot shell, UI, content, deployment, compatibility crates, fixtures and vendor patches |
| kneeman-lines/0_rust_v1_ship | 83085cd0accf6fa8dbc643ea417b73e19749a5db | Original ship, items, drawing and stages; executable reference |
| kneeman-lines/1_gdscript_v3_css | 989412fda250c949da53f6f04b09be57b4d63763 | GDScript mechanics, destruction and debugger reference |
| kneeman-lines/4_melee_decomp | cca1beea (observed checkout) | doldecomp/melee source reference |

The existing pure Kneeman extraction predates this relocation; its receipt is recovery
commit `018568b`, `crates/godot-shell/src/v1`, plus pure input/asset/netplay helpers.

Relocation mapping:

| Recovery path | Destination |
| --- | --- |
| crates/godot-shell | app/crates/godot-shell |
| crates/{html,css,statecharts,world,devtools} | app/crates/{html,css,statecharts,world,devtools} |
| godot, content, vendor/{ggrs,gdext-egui} | app/ with the same relative layout |
| vendor/egui-rsx-macro | ../../crates/egui-rsx-macro |
| deploy/scripts/1_web.mjs | ../../tools/godot-web/1_web.mjs plus app-specific wrapper |
| deploy/{profiles,nginx,web}, deployment contract test | app/deploy/ |
| evidence/row04/traversal-replay.json | app/evidence/row04/traversal-replay.json |

Source filenames are retained to support Git/source comparisons. Dependency paths and
the build fingerprint were adjusted for the new location. Gameplay algorithms were unchanged.

## Behavior references still being developed

Recovery `plans/4_fighter_behavior_inventory.md` inventories implemented fighter behavior.
Recovery `docs/research/melee-paired-parity-ledger.md` has one detailed input entry and
four next inventory entries. It pins `d232a18597b5567304e80d9702e7e2e911a8fdcd`;
its `ft/chara` paths differ from the observed checkout's `ft/kinds` paths.

Recovery `docs/research/brawl-psa-project-m.md` and
`docs/research/tilt-smash-and-buffer-comparison.md` contain PM research and explicit
evidence gaps. Project M is the requested gameplay target. An original-PM checkout,
exact target version and dedicated PM parity ledger remain to be established.
Melee, Brawl and Project+ observations must retain their source/version labels.

Historical generic-engine constraints and architectural prose are preserved in the
source repositories. This receipt does not adopt them as new implementation requirements.

## Single-runtime follow-up

The copied alternate-runtime crates, shell adapters, selector scenes, authored document content,
and obsolete evidence fixture were retired from the active app on 2026-09-06. The temporary
recoverable relocation is /private/tmp/kneeman-retired.LckpBc; durable originals remain in recovery
8950de7e. Presentations, game art, ship receipt and the playable simulation were retained.

Art tools recovered from 0_rust_v1_ship at 83085cd0: tools/pose-capture/{index.html,make_refs.py,
slice_sheet.py}, tools/{roa_search.py,roa_get.py,steam_qr.py,fetch_packs.py,requirements.txt}.
Current adapters and operator commands are documented in ../tools/README.md.
