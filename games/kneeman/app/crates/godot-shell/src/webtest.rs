//! Web test hooks for the Playwright E2E harness (plans/e2e-playwright.md). Everything here is
//! gated behind explicit boot query params -- `?autofind=1` and/or `?script=<base64 json>` on
//! `location.search` -- so normal play never touches any of this: no JS eval, no window global.
//!
//! - `?autofind=1`: fire the same `find_match` the status chip's tap does (`KneeMan::on_connect`),
//!   right at boot. The chip is canvas-drawn (no DOM selector), so this is the harness's way in.
//! - `?script=<base64 json>`: install a `ScriptedDevice` script (see `crate::scripted`) -- a JSON
//!   array of `{start_tick, duration_ticks, frame}` -- that substitutes for the local device poll
//!   at the scripted ticks. `frame` is an `InputFrame` object by field name (see `frame_from_dict`
//!   below for the exact keys); any field it omits stays at `InputFrame::default()` (neutral).
//!   Delivered as base64'd JSON so it rides a query string untouched -- base64's `+ / =` still need
//!   `encodeURIComponent` on the harness side, since we read the value via `URLSearchParams` (which
//!   percent-decodes for us) rather than hand-splitting the raw query string.
//! - `window.__smash`: once either param is present, refreshed every physics tick with a plain
//!   `{tick, checksum, phase}` snapshot for the V1 shell.

#[cfg(target_arch = "wasm32")]
use godot::prelude::*;

use crate::input_frame::InputFrame;
use crate::scripted::ScriptEntry;
use crate::sim::SimState;

thread_local! {
    // The installed script, if any. Empty on every build that never saw `?script=`, so
    // `scripted_frame` is always `None` and `sample_input` falls straight through to the device.
    static SCRIPT: std::cell::RefCell<Vec<ScriptEntry>> = const { std::cell::RefCell::new(Vec::new()) };
    // Set once at boot if `autofind` or `script` was present. Gates the per-tick `window.__smash`
    // refresh so normal play never pays for an eval call it didn't ask for.
    static TEST_MODE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // `?dumpstate=1`: also publish the raw bincode SimState (base64) into `__smash.state` while
    // Running -- a desync debugging aid so a harness can byte-diff two peers, not a test surface.
    static DUMP_STATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // Exact frame at which an evidence capture freezes the SceneTree. u64::MAX means disabled.
    static FREEZE_AT: std::cell::Cell<u64> = const { std::cell::Cell::new(u64::MAX) };
}

/// The scripted frame for `tick`, or `None` if nothing covers it -- the caller then falls back to
/// the real device poll. Cheap and safe to call unconditionally (native/no-script builds just see
/// an empty script and always return `None`).
pub(crate) fn scripted_frame(tick: u64) -> Option<InputFrame> {
    SCRIPT.with(|s| crate::scripted::scripted_frame(&s.borrow(), tick))
}

/// Whether either test param armed at boot. Gates the `window.__smash` refresh.
pub(crate) fn test_mode() -> bool {
    TEST_MODE.with(|t| t.get())
}

/// True once the published sim clock reaches the explicitly requested evidence frame.
pub(crate) fn should_freeze(frame: u64) -> bool {
    FREEZE_AT.with(|at| frame >= at.get())
}

/// Boot hook, called once from `KneeMan::ready`. Reads the test query params (web export only) and
/// arms whatever they ask for. Returns true when `autofind` was requested, so the caller can drive
/// its own `on_connect()` -- this module has no `KneeMan` handle to call it directly.
#[cfg(target_arch = "wasm32")]
pub(crate) fn boot() -> bool {
    let autofind = crate::net::query_param("autofind").as_deref() == Some("1");
    let script = crate::net::query_param("script");
    if let Some(b64) = script.as_ref() {
        install_script(b64);
    }
    if crate::net::query_param("dumpstate").as_deref() == Some("1") {
        DUMP_STATE.with(|t| t.set(true));
    }
    let freeze = install_freeze();
    if autofind || script.is_some() || freeze {
        TEST_MODE.with(|t| t.set(true));
    }
    autofind
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn boot() -> bool {
    false
}

/// Decode `?script=`'s base64 JSON payload and install it. Malformed input just leaves `SCRIPT`
/// empty (the harness's own assertion then fails loudly on "nothing moved", which is diagnosis
/// enough -- this is a test-only path, not one that needs its own error channel).
#[cfg(target_arch = "wasm32")]
fn install_script(b64: &str) {
    let raw = godot::classes::Marshalls::singleton().base64_to_raw(&GString::from(b64));
    let Ok(text) = std::str::from_utf8(raw.as_slice()) else {
        return;
    };
    let entries = parse_script(text);
    SCRIPT.with(|s| *s.borrow_mut() = entries);
}

#[cfg(target_arch = "wasm32")]
fn install_freeze() -> bool {
    let Some(value) = crate::net::query_param("freeze") else {
        return false;
    };
    let Ok(frame) = value.parse::<u64>() else {
        return false;
    };
    FREEZE_AT.with(|at| at.set(frame));
    true
}

/// Parse the `[{start_tick, duration_ticks, frame}, ...]` JSON array via Godot's `Json` (handles
/// escaping for us, same as `rtc::parse_json`). An entry that doesn't fit the shape is dropped
/// rather than aborting the whole script.
#[cfg(target_arch = "wasm32")]
fn parse_script(json: &str) -> Vec<ScriptEntry> {
    let v = godot::classes::Json::parse_string(&GString::from(json));
    let Ok(arr) = v.try_to::<VariantArray>() else {
        return Vec::new();
    };
    arr.iter_shared()
        .filter_map(|item| item.try_to::<Dictionary>().ok())
        .map(|d| ScriptEntry {
            start_tick: crate::rtc::dget_int(&d, "start_tick").max(0) as u64,
            duration_ticks: crate::rtc::dget_int(&d, "duration_ticks").max(0) as u64,
            frame: d
                .get("frame")
                .and_then(|v| v.try_to::<Dictionary>().ok())
                .map(|fd| frame_from_dict(&fd))
                .unwrap_or_default(),
        })
        .collect()
}

/// Read one JSON `frame` object into an `InputFrame`; any field it omits keeps `Default`'s neutral
/// value, so a script only has to spell out what it's actually pressing. Keys match `InputFrame`'s
/// field names 1:1 (see `crate::input_frame::InputFrame`).
#[cfg(target_arch = "wasm32")]
fn frame_from_dict(d: &Dictionary) -> InputFrame {
    let f = |k: &str| d.get(k).and_then(|v| v.try_to::<f64>().ok()).unwrap_or(0.0) as f32;
    let b = |k: &str| {
        d.get(k)
            .and_then(|v| v.try_to::<bool>().ok())
            .unwrap_or(false)
    };
    InputFrame {
        dir: f("dir"),
        aim_y: f("aim_y"),
        cx: f("cx"),
        cy: f("cy"),
        jump: b("jump"),
        jump_held: b("jump_held"),
        shorthop: b("shorthop"),
        shield_held: b("shield_held"),
        shield_pressed: b("shield_pressed"),
        down: b("down"),
        down_pressed: b("down_pressed"),
        attack: b("attack"),
        attack_held: b("attack_held"),
        grab: b("grab"),
        special: b("special"),
    }
}

/// Publish a debug JSON object at `window.__smash.debug` (dumpstate-gated, desync forensics).
#[cfg(target_arch = "wasm32")]
pub(crate) fn debug_json(json: &str) {
    if DUMP_STATE.with(|t| t.get()) {
        crate::net::js_eval(&format!(
            "window.__smash=window.__smash||{{}};window.__smash.debug={json};"
        ));
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn debug_json(_json: &str) {}

/// Refresh `window.__smash = {tick, checksum, phase}`. Called once per physics tick while
/// `test_mode()` is armed; a no-op call is one JS eval of a short literal, cheap enough that this
/// doesn't need its own finer-grained gate.
#[cfg(target_arch = "wasm32")]
pub(crate) fn refresh(tick: u64, phase: &str, running: bool, state: &SimState) {
    let bytes = bincode::serialize(state).expect("serialize SimState for the __smash checksum");
    let digest = crate::asset_wire::asset_id(&bytes).0;
    let checksum = crate::godot_store::hex32(&digest);
    // `at[tick]` pins a checksum to its tick: the harness compares peers AT THE SAME tick (the
    // live `checksum` field alone races -- each page reads it at whatever tick its poll resolved
    // on, and `tick` is serialized inside SimState, so cross-tick values can never match). Only
    // the Running phase records, and any phase change WIPES the map: the offline boot sim counts
    // ticks too, and session start resets SimState to spawn, so without the wipe the map holds a
    // per-page blend of offline-era and net-era values at the same tick numbers. Rolling 600-tick
    // window keeps a long armed session from growing the map without bound.
    let mut record = if running {
        format!("window.__smash.at[{tick}]='{checksum}';delete window.__smash.at[{tick}-600];")
    } else {
        String::new()
    };
    // Pinned to every 60th tick (same-tick states are the only comparable ones, and rollback
    // catch-up skips ticks, so peers intersect their stateAt maps like they do `at`).
    if running && tick % 60 == 0 && DUMP_STATE.with(|t| t.get()) {
        let b64 = godot::classes::Marshalls::singleton()
            .raw_to_base64(&PackedByteArray::from(bytes.as_slice()));
        record.push_str(&format!(
            "window.__smash.stateAt=window.__smash.stateAt||{{}};window.__smash.stateAt[{tick}]='{b64}';"
        ));
    }
    let code = format!(
        "window.__smash=window.__smash||{{at:{{}}}};if(window.__smash.phase!=='{phase}')window.__smash.at={{}};window.__smash.tick={tick};window.__smash.checksum='{checksum}';window.__smash.phase='{phase}';{record}"
    );
    crate::net::js_eval(&code);
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn refresh(_tick: u64, _phase: &str, _running: bool, _state: &SimState) {}
