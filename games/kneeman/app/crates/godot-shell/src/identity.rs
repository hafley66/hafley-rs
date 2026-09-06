//! Local player identity: nametag text + color, the per-slot color/name defaults, and persistence
//! to `user://identity.cfg` (browser localStorage on web). Cosmetic only -- never rolled back, never
//! part of the netplay checksum. Lifted out of the `KneeMan` node.

use godot::prelude::*;

/// Local player presentation: the nametag text + color the sprite and tag wear. NOT sim state
/// (purely cosmetic, never rolled back). Lives on the node as a `Mutable` so the debug panel's
/// Identity tab edits it and the renderer reads it. Persisted to/from browser localStorage on the
/// web build; defaults on desktop.
#[derive(Clone, PartialEq)]
pub struct Identity {
    pub name: String,
    pub color: Color,
    pub font_px: i32, // nametag font size (HUD-wide; both tags share the local player's setting)
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            name: "Player".into(),
            color: Color::from_rgb(0.35, 0.75, 1.0),
            font_px: 32,
        }
    }
}

/// Presentation color for a non-local fighter (slot 0 wears the local identity color instead).
/// Cosmetic only — never folded into the netplay checksum, so it can never desync a session.
pub(crate) fn slot_color(idx: usize) -> Color {
    match idx {
        1 => Color::from_rgb(1.0, 0.55, 0.35),  // orange
        2 => Color::from_rgb(0.45, 0.92, 0.50), // green
        3 => Color::from_rgb(0.80, 0.55, 0.95), // purple
        _ => Color::from_rgb(0.35, 0.75, 1.0),  // blue (slot-0 fallback)
    }
}

/// Nametag text for a non-local fighter ("P2".."P4"); slot 0 wears the local identity name.
pub(crate) fn slot_name(idx: usize) -> String {
    format!("P{}", idx + 1)
}

/// Cap the name length and trim; cosmetic only (the tag and the saved file both show this).
pub(crate) fn sanitize_name(s: &str) -> String {
    let t: String = s.chars().take(16).collect();
    let t = t.trim();
    if t.is_empty() {
        "Player".into()
    } else {
        t.to_string()
    }
}

/// Identity persistence. `user://` is Godot's per-user store: a real file on native, IndexedDB on
/// the web export — Godot bridges the platform difference, so one code path covers both (no
/// `JavaScriptBridge`, no platform `cfg`). `ConfigFile` serializes Variants, so the `Color` round-
/// trips natively without any hex conversion.
const IDENTITY_PATH: &str = "user://identity.cfg";

pub(crate) fn load_identity() -> Identity {
    let mut id = Identity::default();
    let mut cfg = godot::classes::ConfigFile::new_gd();
    if cfg.load(IDENTITY_PATH) != godot::global::Error::OK {
        return id;
    }
    if let Ok(g) = cfg.get_value("player", "name").try_to::<GString>() {
        let s = g.to_string();
        if !s.is_empty() {
            id.name = sanitize_name(&s);
        }
    }
    if let Ok(c) = cfg.get_value("player", "color").try_to::<Color>() {
        id.color = c;
    }
    if let Ok(px) = cfg.get_value("player", "font_px").try_to::<i64>() {
        id.font_px = (px as i32).clamp(10, 96);
    }
    id
}

pub(crate) fn save_identity(id: &Identity) {
    let mut cfg = godot::classes::ConfigFile::new_gd();
    let _ = cfg.load(IDENTITY_PATH); // keep any other keys already on disk
    cfg.set_value(
        "player",
        "name",
        &GString::from(sanitize_name(&id.name).as_str()).to_variant(),
    );
    cfg.set_value("player", "color", &id.color.to_variant());
    cfg.set_value("player", "font_px", &(id.font_px as i64).to_variant());
    cfg.save(IDENTITY_PATH);
}

/// Client-local UI scale (bigger = bigger egui UI). Cosmetic + per-device, so it lives in the same
/// `user://` config as identity, NOT in `Tune` (which is netplay-hashed). Applied INVERSELY to the
/// egui `pixels_per_point` counter-scale in `ui/debug.rs` (higher ppp = smaller UI there), so a bad
/// auto-detect on any platform is always a slider-drag from correct. Default + clamp are generous.
pub(crate) fn load_ui_scale() -> f32 {
    let mut cfg = godot::classes::ConfigFile::new_gd();
    if cfg.load(IDENTITY_PATH) != godot::global::Error::OK {
        return 1.0;
    }
    cfg.get_value("display", "ui_scale")
        .try_to::<f64>()
        .map(|v| (v as f32).clamp(0.4, 4.0))
        .unwrap_or(1.0)
}

pub(crate) fn save_ui_scale(scale: f32) {
    let mut cfg = godot::classes::ConfigFile::new_gd();
    let _ = cfg.load(IDENTITY_PATH); // keep name/color/font/char keys already on disk
    cfg.set_value(
        "display",
        "ui_scale",
        &(scale.clamp(0.4, 4.0) as f64).to_variant(),
    );
    cfg.save(IDENTITY_PATH);
}

/// Saved roster picks (both hotseat slots), same file as the identity. Raw MENU values —
/// `sync_charsel` clamps against the live roster, so a stale index from a shrunk roster
/// just clamps instead of breaking the load.
pub(crate) fn load_charsel() -> [i64; 2] {
    let mut out = [0, 1];
    let mut cfg = godot::classes::ConfigFile::new_gd();
    if cfg.load(IDENTITY_PATH) != godot::global::Error::OK {
        return out;
    }
    for (k, key) in ["char0", "char1"].into_iter().enumerate() {
        if let Ok(v) = cfg.get_value("player", key).try_to::<i64>() {
            out[k] = v.max(0);
        }
    }
    out
}

pub(crate) fn save_charsel(sel: [i64; 2]) {
    let mut cfg = godot::classes::ConfigFile::new_gd();
    let _ = cfg.load(IDENTITY_PATH); // keep name/color/font keys
    cfg.set_value("player", "char0", &sel[0].to_variant());
    cfg.set_value("player", "char1", &sel[1].to_variant());
    cfg.save(IDENTITY_PATH);
}
