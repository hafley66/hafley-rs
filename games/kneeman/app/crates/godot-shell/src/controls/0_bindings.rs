//! Keyboard remapping at the device boundary. Godot owns event matching and persistence;
//! simulation/replay values contain no physical keys. Pad events survive keyboard edits.
use godot::classes::{ConfigFile, Input, InputEvent, InputEventKey, InputMap};
use godot::global::{Key, KeyLocation};
use godot::prelude::*;
use std::cell::{Cell, RefCell};

const PATH: &str = "user://controls.cfg";

fn saved_key(values: &[i32]) -> Option<(Key, KeyLocation)> {
    let [code, location] = *values else { return None; };
    let key = Key::try_from_ord(code)?;
    let location = KeyLocation::try_from_ord(location)?;
    (key != Key::NONE && !key.as_str().is_empty()).then_some((key, location))
}
thread_local! {
    static DEFAULTS: RefCell<Vec<(&'static str, Array<Gd<InputEvent>>)>> = const { RefCell::new(Vec::new()) };
    static PENDING: Cell<Option<&'static str>> = const { Cell::new(None) };
    static CANCELLED: Cell<bool> = const { Cell::new(false) };
    static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn load() {
    if DEFAULTS.with_borrow(|rows| !rows.is_empty()) { return; }
    let mut map = InputMap::singleton();
    for (name, key) in [("aim_up", Key::UP), ("aim_down", Key::DOWN),
        ("aim_left", Key::LEFT), ("aim_right", Key::RIGHT)] {
        if !map.has_action(name) {
            map.add_action(name);
            let mut event = InputEventKey::new_gd();
            event.set_physical_keycode(key);
            map.action_add_event(name, &event.upcast::<InputEvent>());
        }
    }
    let mut cfg = ConfigFile::new_gd();
    let loaded = cfg.load(PATH) == godot::global::Error::OK;
    for row in super::P1_MANUAL {
        for &name in row.keyboard {
            DEFAULTS.with_borrow_mut(|rows| rows.push((name, map.action_get_events(name))));
            if loaded && cfg.has_section_key("keys", name) {
                let saved = cfg.get_value("keys", name).try_to::<PackedInt32Array>();
                if let Ok(saved) = saved
                    && let Some((key, location)) = saved_key(saved.as_slice()) {
                    let mut event = InputEventKey::new_gd();
                    event.set_physical_keycode(key);
                    event.set_location(location);
                    replace(name, &event.upcast());
                } else {
                    STATUS.with_borrow_mut(|s| *s = format!("Ignored invalid saved binding: {name}"));
                }
            }
        }
    }
}

fn replace(name: &str, event: &Gd<InputEvent>) {
    let mut map = InputMap::singleton();
    for previous in map.action_get_events(name).iter_shared() {
        if previous.clone().try_cast::<InputEventKey>().is_ok() {
            map.action_erase_event(name, &previous);
        }
    }
    map.action_add_event(name, event);
    Input::singleton().action_release(name);
}

pub fn label(name: &str) -> String {
    InputMap::singleton().action_get_events(name).iter_shared()
        .filter_map(|event| event.try_cast::<InputEventKey>().ok())
        .map(|event| format!("{}{}", match event.get_location() {
            KeyLocation::LEFT => "L ", KeyLocation::RIGHT => "R ", _ => "",
        }, event.as_text_physical_keycode()))
        .collect::<Vec<_>>().join(" / ")
}

pub fn begin(name: &'static str) { PENDING.set(Some(name)); }
pub fn cancel() { PENDING.set(None); }
pub fn pending() -> Option<&'static str> { PENDING.get() }
pub fn take_cancelled() -> bool { CANCELLED.replace(false) }
pub fn status() -> String { STATUS.with_borrow(Clone::clone) }

/// Called before gameplay event handling. Escape cancels; repeats/releases never bind.
pub fn capture(event: &Gd<InputEvent>) -> bool {
    let Some(name) = PENDING.get() else { return false; };
    let Ok(key) = event.clone().try_cast::<InputEventKey>() else { return false; };
    if !key.is_pressed() || key.is_echo() { return true; }
    PENDING.set(None);
    if key.get_keycode() == Key::ESCAPE { CANCELLED.set(true); return true; }
    let code = key.get_physical_keycode();
    if saved_key(&[code.ord(), key.get_location().ord()]).is_none() {
        STATUS.with_borrow_mut(|s| *s = "Unsupported physical key; binding unchanged.".into());
        return true;
    }
    let mut binding = InputEventKey::new_gd();
    binding.set_physical_keycode(code);
    binding.set_location(key.get_location());
    replace(name, &binding.upcast());
    let mut cfg = ConfigFile::new_gd();
    let _ = cfg.load(PATH);
    cfg.set_value("keys", name, &PackedInt32Array::from(&[code.ord(), key.get_location().ord()]).to_variant());
    let result = cfg.save(PATH);
    STATUS.with_borrow_mut(|s| *s = if result == godot::global::Error::OK {
        format!("Saved {name}. Shared keys trigger every bound action.")
    } else { format!("Binding applied for this session; save failed: {result:?}") });
    true
}

pub fn reset() {
    cancel();
    DEFAULTS.with_borrow(|rows| {
        for (name, events) in rows {
            let mut map = InputMap::singleton();
            for event in map.action_get_events(*name).iter_shared() {
                if event.clone().try_cast::<InputEventKey>().is_ok() { map.action_erase_event(*name, &event); }
            }
            for event in events.iter_shared() {
                if event.clone().try_cast::<InputEventKey>().is_ok() { map.action_add_event(*name, &event); }
            }
            Input::singleton().action_release(*name);
        }
    });
    let mut cfg = ConfigFile::new_gd();
    let _ = cfg.load(PATH);
    if cfg.has_section("keys") { cfg.erase_section("keys"); }
    let result = cfg.save(PATH);
    STATUS.with_borrow_mut(|s| *s = format!("Keyboard defaults restored; save: {result:?}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_keys_preserve_side_and_reject_malformed_or_unknown_values() {
        for key in [Key::A, Key::SHIFT, Key::LEFT] {
            for location in [KeyLocation::UNSPECIFIED, KeyLocation::LEFT, KeyLocation::RIGHT] {
                assert_eq!(saved_key(&[key.ord(), location.ord()]), Some((key, location)));
            }
        }
        for invalid in [&[][..], &[65], &[65, 0, 9], &[0, 0], &[-1, 0],
            &[i32::MAX, 0], &[65, -1], &[65, 3]] {
            assert_eq!(saved_key(invalid), None, "{invalid:?}");
        }
    }
}
