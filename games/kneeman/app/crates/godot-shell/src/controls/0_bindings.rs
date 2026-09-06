//! Keyboard/gamepad remapping at the device boundary. Godot owns event matching and persistence;
//! simulation/replay values contain no physical keys. Pad events survive keyboard edits.
use godot::classes::{ConfigFile, Input, InputEvent, InputEventKey, InputEventJoypadButton, InputEventJoypadMotion, InputMap};
use godot::global::{Key, KeyLocation, JoyAxis, JoyButton};
use godot::prelude::*;
use std::cell::{Cell, RefCell};

const PATH: &str = "user://controls.cfg";
pub const PAD_ACTIONS: &[(&str, &str)] = &[
    ("jump", "p2_jump"), ("shorthop", "p2_shorthop"), ("attack", "p2_attack"),
    ("shield", "p2_shield"), ("grab", "p2_grab"), ("special", "p2_special"),
    ("pause", "p2_pause"),
];
pub const PAD_STICKS: &[(&str, &str)] = &[
    ("pad_left", "p2_pad_left"), ("pad_right", "p2_pad_right"),
    ("pad_up", "p2_pad_up"), ("pad_down", "p2_pad_down"),
    ("pad_aim_left", "p2_pad_aim_left"), ("pad_aim_right", "p2_pad_aim_right"),
    ("pad_aim_up", "p2_pad_aim_up"), ("pad_aim_down", "p2_pad_aim_down"),
];
pub const PAD_DPAD: &[(&str, &str)] = &[
    ("pad_dpad_left", "p2_pad_dpad_left"), ("pad_dpad_right", "p2_pad_dpad_right"),
    ("pad_dpad_up", "p2_pad_dpad_up"), ("pad_dpad_down", "p2_pad_dpad_down"),
];

fn saved_pad(values: &[i32]) -> Option<(i32, i32, i32)> {
    let [kind, index, sign] = *values else { return None; };
    match kind {
        0 if (0..JoyButton::MAX.ord()).contains(&index) && sign == 0 => Some((kind, index, sign)),
        1 if (0..JoyAxis::MAX.ord()).contains(&index) && [-1, 1].contains(&sign) => Some((kind, index, sign)),
        _ => None,
    }
}
fn pad_event(values: &[i32]) -> Option<Gd<InputEvent>> {
    saved_pad(values)?;
    match *values {
        [0, button, 0] if (0..JoyButton::MAX.ord()).contains(&button) => {
            let mut event = InputEventJoypadButton::new_gd();
            event.set_button_index(JoyButton::try_from_ord(button)?);
            Some(event.upcast())
        }
        [1, axis, sign] if (0..JoyAxis::MAX.ord()).contains(&axis) && [-1, 1].contains(&sign) => {
            let mut event = InputEventJoypadMotion::new_gd();
            event.set_axis(JoyAxis::try_from_ord(axis)?);
            event.set_axis_value(sign as f32);
            Some(event.upcast())
        }
        _ => None,
    }
}
fn is_pad(event: &Gd<InputEvent>) -> bool {
    event.clone().try_cast::<InputEventJoypadButton>().is_ok()
        || event.clone().try_cast::<InputEventJoypadMotion>().is_ok()
}

// The web filesystem flushes asynchronously. Small settings use synchronous browser storage;
// ConfigFile still owns serialization, and existing user:// bindings migrate on the next edit.
fn read_config(cfg: &mut Gd<ConfigFile>) -> godot::global::Error {
    #[cfg(target_arch = "wasm32")]
    {
        let text = crate::net::js_eval("(() => { try { return localStorage.getItem(location.pathname + ':controls:v1'); } catch (_) { return null; } })()")
            .try_to::<GString>();
        if let Ok(text) = text { return cfg.parse(&text); }
    }
    cfg.load(PATH)
}

fn write_config(cfg: &mut Gd<ConfigFile>) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let text = serde_json::to_string(&cfg.encode_to_text().to_string()).map_err(|e| e.to_string())?;
        let result = crate::net::js_eval(&format!("(() => {{ try {{ localStorage.setItem(location.pathname + ':controls:v1', {text}); return ''; }} catch (e) {{ return String(e); }} }})()"))
            .try_to::<GString>().map_err(|_| "Browser storage unavailable".to_string())?;
        if result.is_empty() { Ok(()) } else { Err(result.to_string()) }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = cfg.save(PATH);
        if result == godot::global::Error::OK { Ok(()) } else { Err(format!("{result:?}")) }
    }
}

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
    static PAD_DEFAULTS: RefCell<Vec<(&'static str, Array<Gd<InputEvent>>)>> = const { RefCell::new(Vec::new()) };
    static PAD_PLAYER: Cell<Option<usize>> = const { Cell::new(None) };
}

pub fn load() {
    if DEFAULTS.with_borrow(|rows| !rows.is_empty()) { return; }
    let mut map = InputMap::singleton();
    for (name, key) in [("aim_up", Key::UP), ("aim_down", Key::DOWN),
        ("aim_left", Key::LEFT), ("aim_right", Key::RIGHT), ("pause", Key::ESCAPE)] {
        if !map.has_action(name) {
            map.add_action(name);
            let mut event = InputEventKey::new_gd();
            event.set_physical_keycode(key);
            map.action_add_event(name, &event.upcast::<InputEvent>());
        }
    }
    let mut cfg = ConfigFile::new_gd();
    let loaded = read_config(&mut cfg) == godot::global::Error::OK;
    // P2 gets its own action names and independent resources. Both pads keep the
    // existing defaults, including R2 attack, while InputMap owns event matching.
    map.action_add_event("attack", &pad_event(&[1, JoyAxis::TRIGGER_RIGHT.ord(), 1]).unwrap());
    map.action_add_event("pause", &pad_event(&[0, JoyButton::START.ord(), 0]).unwrap());
    for (i, &(name, _)) in PAD_STICKS.iter().enumerate() {
        map.add_action_ex(name).deadzone(0.0).done();
        let axis = [JoyAxis::LEFT_X, JoyAxis::LEFT_Y, JoyAxis::RIGHT_X, JoyAxis::RIGHT_Y][i / 2];
        map.action_add_event(name, &pad_event(&[1, axis.ord(), if i % 2 == 0 { -1 } else { 1 }]).unwrap());
    }
    for (i, &(name, _)) in PAD_DPAD.iter().enumerate() {
        map.add_action_ex(name).deadzone(0.5).done();
        let button = [JoyButton::DPAD_LEFT, JoyButton::DPAD_RIGHT, JoyButton::DPAD_UP, JoyButton::DPAD_DOWN][i];
        map.action_add_event(name, &pad_event(&[0, button.ord(), 0]).unwrap());
    }
    for &(p1, p2) in PAD_ACTIONS.iter().chain(PAD_STICKS).chain(PAD_DPAD) {
        map.add_action_ex(p2).deadzone(0.5).done();
        for event in map.action_get_events(p1).iter_shared().filter(is_pad) {
            map.action_add_event(p2, &event.duplicate().unwrap().cast::<InputEvent>());
        }
        for name in [p1, p2] {
            PAD_DEFAULTS.with_borrow_mut(|rows| rows.push((name, map.action_get_events(name))));
            if loaded && cfg.has_section_key("pads", name) {
                let saved = cfg.get_value("pads", name).try_to::<PackedInt32Array>().ok();
                if let Some(event) = saved.and_then(|v| pad_event(v.as_slice())) {
                    replace_pad(name, &event);
                } else { STATUS.with_borrow_mut(|s| *s = format!("Ignored invalid saved pad binding: {name}")); }
            }
        }
    }
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
    sync_pads();
    Input::singleton().connect("joy_connection_changed", &Callable::from_fn(
        "sync_pads", |_| { sync_pads(); },
    ));
}

// Bind gameplay events to each player's connected device; absent pads match no device.
fn sync_pads() {
    let mut map = InputMap::singleton();
    for &(p1, p2) in PAD_ACTIONS.iter().chain(PAD_STICKS).chain(PAD_DPAD) {
        for (player, name) in [p1, p2].into_iter().enumerate() {
            let device = Input::singleton().get_connected_joypads().get(player)
                .map(|id| id as i32).unwrap_or(i32::MAX);
            for mut event in map.action_get_events(name).iter_shared() {
                if event.clone().try_cast::<InputEventJoypadButton>().is_ok()
                    || event.clone().try_cast::<InputEventJoypadMotion>().is_ok() {
                    map.action_erase_event(name, &event);
                    event.set_device(device);
                    map.action_add_event(name, &event);
                }
            }
        }
    }
    super::release_all();
}

fn replace_pad(name: &str, event: &Gd<InputEvent>) {
    let mut map = InputMap::singleton();
    for previous in map.action_get_events(name).iter_shared().filter(is_pad) {
        map.action_erase_event(name, &previous);
    }
    map.action_add_event(name, event);
    Input::singleton().action_release(name);
}

pub fn pad_label(name: &str) -> String {
    InputMap::singleton().action_get_events(name).iter_shared().filter(is_pad)
        .map(|event| {
            if let Ok(button) = event.clone().try_cast::<InputEventJoypadButton>() {
                match button.get_button_index() {
                    JoyButton::A => "A".into(), JoyButton::B => "B".into(),
                    JoyButton::X => "X".into(), JoyButton::Y => "Y".into(),
                    JoyButton::BACK => "Back".into(), JoyButton::LEFT_SHOULDER => "L1".into(),
                    JoyButton::RIGHT_SHOULDER => "R1".into(),
                    JoyButton::START => "Start".into(),
                    JoyButton::DPAD_LEFT => "D-pad left".into(), JoyButton::DPAD_RIGHT => "D-pad right".into(),
                    JoyButton::DPAD_UP => "D-pad up".into(), JoyButton::DPAD_DOWN => "D-pad down".into(),
                    other => format!("Button {}", other.ord()),
                }
            } else {
                let axis = event.cast::<InputEventJoypadMotion>();
                format!("Axis {}{}", axis.get_axis().ord(), if axis.get_axis_value() > 0.0 { "+" } else { "-" })
            }
        }).collect::<Vec<_>>().join(" / ")
}
pub fn begin_pad(name: &'static str, player: usize) { PENDING.set(Some(name)); PAD_PLAYER.set(Some(player)); }
pub fn pending_player() -> Option<usize> { PAD_PLAYER.get() }

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

pub fn begin(name: &'static str) { PENDING.set(Some(name)); PAD_PLAYER.set(None); }
pub fn cancel() { PENDING.set(None); PAD_PLAYER.set(None); }
pub fn pending() -> Option<&'static str> { PENDING.get() }
pub fn take_cancelled() -> bool { CANCELLED.replace(false) }
pub fn status() -> String { STATUS.with_borrow(Clone::clone) }

/// UI intent, outside the semantic simulation packet. Capture/cancel must not also navigate.
pub fn menu_pressed() -> bool {
    PENDING.get().is_none() && !CANCELLED.get()
        && ["pause", "p2_pause"].iter().any(|name| Input::singleton().is_action_just_pressed(*name))
}

/// Called before gameplay event handling. Escape cancels; repeats/releases never bind.
pub fn capture(event: &Gd<InputEvent>) -> bool {
    let Some(name) = PENDING.get() else { return false; };
    if let Some(player) = PAD_PLAYER.get() {
        if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
            if key.is_pressed() && key.get_keycode() == Key::ESCAPE { cancel(); CANCELLED.set(true); }
            return true;
        }
        let device = Input::singleton().get_connected_joypads().get(player).map(|d| d as i32);
        if Some(event.get_device()) != device { return true; }
        let values = if let Ok(button) = event.clone().try_cast::<InputEventJoypadButton>() {
            if !button.is_pressed() { return true; }
            [0, button.get_button_index().ord(), 0]
        } else if let Ok(axis) = event.clone().try_cast::<InputEventJoypadMotion>() {
            if axis.get_axis_value().abs() < 0.75 { return true; }
            [1, axis.get_axis().ord(), if axis.get_axis_value() > 0.0 { 1 } else { -1 }]
        } else { return false; };
        if let Some(binding) = pad_event(&values) {
            replace_pad(name, &binding); sync_pads(); cancel();
            let mut cfg = ConfigFile::new_gd(); let _ = read_config(&mut cfg);
            cfg.set_value("pads", name, &PackedInt32Array::from(&values).to_variant());
            STATUS.with_borrow_mut(|s| *s = match write_config(&mut cfg) {
                Ok(()) => format!("Saved {name}. Shared controls trigger every bound action."),
                Err(e) => format!("Pad binding applied for this session; save failed: {e}"),
            });
        }
        return true;
    }
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
    let _ = read_config(&mut cfg);
    cfg.set_value("keys", name, &PackedInt32Array::from(&[code.ord(), key.get_location().ord()]).to_variant());
    STATUS.with_borrow_mut(|s| *s = match write_config(&mut cfg) {
        Ok(()) => format!("Saved {name}. Shared keys trigger every bound action."),
        Err(error) => format!("Binding applied for this session; save failed: {error}"),
    });
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
    let _ = read_config(&mut cfg);
    if cfg.has_section("keys") { cfg.erase_section("keys"); }
    STATUS.with_borrow_mut(|s| *s = match write_config(&mut cfg) {
        Ok(()) => "Keyboard defaults restored and saved.".into(),
        Err(error) => format!("Keyboard defaults restored for this session; save failed: {error}"),
    });
}

pub fn reset_pads() {
    cancel();
    let mut map = InputMap::singleton();
    PAD_DEFAULTS.with_borrow(|rows| for (name, events) in rows {
        for event in map.action_get_events(*name).iter_shared().filter(is_pad) { map.action_erase_event(*name, &event); }
        for event in events.iter_shared().filter(is_pad) {
            map.action_add_event(*name, &event.duplicate().unwrap().cast::<InputEvent>());
        }
    });
    sync_pads();
    let mut cfg = ConfigFile::new_gd(); let _ = read_config(&mut cfg);
    if cfg.has_section("pads") { cfg.erase_section("pads"); }
    STATUS.with_borrow_mut(|s| *s = match write_config(&mut cfg) {
        Ok(()) => "Gamepad defaults restored and saved.".into(),
        Err(e) => format!("Gamepad defaults restored for this session; save failed: {e}"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pad_values_validate_kind_range_and_direction() {
        for button in 0..JoyButton::MAX.ord() { assert_eq!(saved_pad(&[0, button, 0]), Some((0, button, 0))); }
        for axis in 0..JoyAxis::MAX.ord() { for sign in [-1, 1] {
            assert_eq!(saved_pad(&[1, axis, sign]), Some((1, axis, sign)));
        }}
        for invalid in [&[][..], &[0, 1], &[0, -1, 0], &[0, JoyButton::MAX.ord(), 0],
            &[0, 1, 1], &[1, -1, 1], &[1, JoyAxis::MAX.ord(), 1], &[1, 0, 0], &[1, 0, 2], &[2, 0, 0]] {
            assert_eq!(saved_pad(invalid), None, "{invalid:?}");
        }
    }

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
