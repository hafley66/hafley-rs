#![cfg(feature = "ingest")]

use base64::Engine;
use game_content::{bake, decode_file, decode_html};
use std::path::Path;

fn fixture(file: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../blender-godot-sqlite-proof/fixtures/falcon")
        .join(file)
}

#[test]
fn retained_actions_preserve_decode_and_baked_wire_shape() {
    let files = [
        ("4_pm36_Wait1.html", "Wait1"),
        ("5_pm36_JumpF.html", "JumpF"),
        ("1_pm36_AttackAirF.html", "AttackAirF"),
        ("6_pm36_JumpSquat.html", "JumpSquat"),
        ("7_pm36_Fall.html", "Fall"),
        ("8_pm36_LandingAirF.html", "LandingAirF"),
        ("9_pm36_LandingHeavy.html", "LandingHeavy"),
    ];
    let actions: Vec<_> = files.iter().map(|(file, _)| decode_file(&fixture(file)).unwrap()).collect();
    assert_eq!(actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        files.iter().map(|(_, name)| *name).collect::<Vec<_>>());
    let baked = bake(&actions);
    for (source, output) in actions.iter().zip(&baked) {
        assert!(!source.frames.is_empty());
        assert_eq!((output.iasa, output.landing_lag, output.frames.len()),
            (source.iasa, source.landing_lag, source.frames.len()));
        let bytes = bincode::serde::encode_to_vec(source, bincode::config::standard()).unwrap();
        let html = format!("const fighter_subaction_data = \"{}\";",
            base64::engine::general_purpose::STANDARD.encode(bytes));
        assert_eq!(serde_json::to_value(decode_html(&html).unwrap()).unwrap(),
            serde_json::to_value(source).unwrap());
    }
    let bytes = serde_json::to_vec(&baked).unwrap();
    let restored: Vec<game_content::Action> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    assert!(baked[2].frames.iter().flat_map(|f| &f.hit_boxes).any(|h| h.damage == 18.0));
}

#[test]
fn rejects_malformed_and_trailing_payloads() {
    for (html, message) in [
        ("<html></html>", "missing subaction data"),
        ("const fighter_subaction_data = \"abc", "unterminated subaction data"),
    ] {
        assert_eq!(decode_html(html).unwrap_err().to_string(), message);
    }
    assert!(decode_html("const fighter_subaction_data = \"!\";").is_err());
    assert!(decode_html("const fighter_subaction_data = \"AA==\";").is_err());
    let source = decode_file(&fixture("4_pm36_Wait1.html")).unwrap();
    let mut bytes = bincode::serde::encode_to_vec(&source, bincode::config::standard()).unwrap();
    bytes.push(0);
    let html = format!("const fighter_subaction_data = \"{}\";",
        base64::engine::general_purpose::STANDARD.encode(bytes));
    assert_eq!(decode_html(&html).unwrap_err().to_string(), "schema must consume the entire payload");
}

#[test]
fn synthetic_identity_is_not_a_fighter_whitelist() {
    // Synthetic mutation verifies name independence, not another real character.
    let mut source = decode_file(&fixture("4_pm36_Wait1.html")).unwrap();
    source.name = "SyntheticAction".into();
    let bytes = bincode::serde::encode_to_vec(&source, bincode::config::standard()).unwrap();
    let html = format!("const fighter_subaction_data = \"{}\";",
        base64::engine::general_purpose::STANDARD.encode(bytes));
    let decoded = decode_html(&html).unwrap();
    assert_eq!(decoded.name, "SyntheticAction");
    assert_eq!(serde_json::to_value(bake(&[decoded])).unwrap(),
        serde_json::to_value(bake(&[source])).unwrap());
}
