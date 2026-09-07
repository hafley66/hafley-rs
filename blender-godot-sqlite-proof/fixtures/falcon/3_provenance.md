# Public Falcon assets

Retrieved 2026-09-07 for local lab inspection. Asset redistribution permissions
are not established by the availability of these downloads.

| Local file | Source | SHA256 |
| --- | --- | --- |
| 0_brawl_falcon.zip | https://models.spriters-resource.com/media/assets/283/286448.zip?updated=1755497015 | d64cde3388b499aa0be640e6ec1cf30cf3558d1b143d0b1c18999c7c6a9d2221 |
| 1_pm36_AttackAirF.html | https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/AttackAirF.html | 01b63dfbd97f6cc4c1536f809809164c514adc9fc2b1633f76bc6e22ffe690db |
| 2_pm36_AttackAirF.gif | https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/AttackAirF.gif | 19b714a406414227bf99c612971b824298d667b4b7a2ba4ed931f4addcc224f4 |
| 4_pm36_Wait1.html | https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/Wait1.html | b5d904b01c4c35630801de7062ed2d495904abb1b4da9d11e97c6dee38f29207 |
| 5_pm36_JumpF.html | https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/JumpF.html | 37a49314e4b7c8b22edd7705238e78f6b5c5cbb77e79d48d4aba882264d51d8a |

Model landing page: https://models.spriters-resource.com/wii/ssbb/asset/286448/
Archive contains 93 files including FitCaptain00.dae and alternate interchange
formats. It is the Brawl model; PM-specific scaling/equivalence remains unverified.

The PM 3.6 forward-air page includes `fighter_subaction_data`, an inline Base64
payload. Existing Rust implementation in rukai/rukaidata's
`fighter_renderer/src/lib.rs` decodes Base64, calls
`bincode::serde::decode_from_slice` with standard configuration into
`brawllib_rs::high_level_fighter::HighLevelSubaction`, then creates the renderer
with `App::new_insert_into_element` and calls `app.run()`.

Source: https://github.com/rukai/rukaidata/blob/main/fighter_renderer/src/lib.rs
Local source: /Users/chrishafley/projects/games/smash/vendor/rukaidata/fighter_renderer/src/lib.rs

The three payloads decode completely with brawllib_rs 0.29.0 and bincode 2.0.1:
Wait1 61 frames / 69,874 bytes, JumpF 36 frames / 41,833 bytes, AttackAirF 40
frames / 51,400 bytes. See `../../falcon-lab` for executed tests and native capture.
Downloaded GIF is upstream reference output, not a recording of this lab.
Full Falcon action collection, a Melee comparison model, and Blender/Godot import
of this archive have not been downloaded or executed in this step.
