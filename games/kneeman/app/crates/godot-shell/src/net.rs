//! Stateless netplay support: the `SimState` snapshot codec (bincode + base64 via Marshalls), room-
//! code minting, the monotonic clock, the transport-state name formatters, and `NetDebug` -- the
//! read-only snapshot the debug panel renders. The live transport state machine (`Phase`, `Room`,
//! `SigCounts`) stays on the `KneeMan` node; this module is only the parts with no node coupling.

use godot::classes::web_rtc_data_channel::ChannelState;
use godot::classes::web_rtc_peer_connection::{ConnectionState, GatheringState, SignalingState};
use godot::classes::web_socket_peer::State as WsState;
use godot::prelude::*;

use crate::sim::{SimState, Tune};

/// Monotonic ms clock (Godot's, so it works the same on native + the emscripten web build).
pub(crate) fn now_ms() -> u64 {
    godot::classes::Time::singleton().get_ticks_msec()
}

/// Mint a private room code for reconnect. Only the host calls this, once, so it just needs to be
/// unlikely to collide with another pair's room at the same instant: microsecond clock xor'd with a
/// hash of the host's name. Both peers then share THIS code (host sends it over the relay).
pub(crate) fn mint_room_code(name: &str) -> String {
    let t = godot::classes::Time::singleton().get_ticks_usec();
    let salt = name
        .bytes()
        .fold(0u64, |a, b| a.wrapping_mul(131).wrapping_add(b as u64));
    format!("rm{:x}", t ^ salt.rotate_left(17))
}

/// Serialize a sim snapshot for the signaling channel: bincode bytes (same wire as ggrs messages),
/// base64'd via Godot's Marshalls so it rides inside a JSON text frame.
pub(crate) fn encode_state(s: &SimState) -> GString {
    let bytes = bincode::serialize(s).expect("serialize SimState snapshot");
    godot::classes::Marshalls::singleton().raw_to_base64(&PackedByteArray::from(bytes.as_slice()))
}

/// Inverse of `encode_state`. `None` if the base64/bincode doesn't decode (malformed frame), so the
/// guest keeps waiting for a good one rather than resuming from garbage.
pub(crate) fn decode_state(b64: &str) -> Option<SimState> {
    let raw = godot::classes::Marshalls::singleton().base64_to_raw(b64);
    bincode::deserialize::<SimState>(raw.as_slice()).ok()
}

/// Serialize the ruleset (`Tune`) for the signaling channel, same wire as the state snapshot. The
/// host is authoritative: it ships this at match start so both peers' reducers run identical physics
/// (custom rulesets in netplay), instead of each side silently using its own Feel-edited `Tune`.
pub(crate) fn encode_tune(t: &Tune) -> GString {
    let bytes = bincode::serialize(t).expect("serialize Tune");
    godot::classes::Marshalls::singleton().raw_to_base64(&PackedByteArray::from(bytes.as_slice()))
}

/// Inverse of `encode_tune`. `None` on a malformed frame, so the guest keeps its own `Tune` and waits
/// for a good one rather than adopting garbage physics.
pub(crate) fn decode_tune(b64: &str) -> Option<Tune> {
    let raw = godot::classes::Marshalls::singleton().base64_to_raw(b64);
    bincode::deserialize::<Tune>(raw.as_slice()).ok()
}

// --- Web-push bridge --------------------------------------------------------------------------
// The lobby push opt-in lives in JS (`deploy/web/push.js` exposes `window.smashPush`); the Network
// page drives it through Godot's `JavaScriptBridge` singleton. gdext 0.4.5's codegen does NOT emit a
// `JavaScriptBridge` class type (even on the emscripten target), so we can't name it -- instead fetch
// the singleton dynamically by name and `call("eval", ...)` on the untyped Object. The singleton is
// only registered on the web export, so `get_singleton` returns None on native -> both are no-ops.

/// The `JavaScriptBridge` engine singleton, or None when not on the web export (native, or a headless
/// run). Untyped because the class isn't in the gdext bindings; we reach `eval` by dynamic `call`.
#[cfg(target_arch = "wasm32")]
fn js_bridge() -> Option<Gd<Object>> {
    godot::classes::Engine::singleton().get_singleton("JavaScriptBridge")
}

/// `JavaScriptBridge.eval(code, /*use_global_execution_context=*/true)` via dynamic dispatch, or Nil
/// when the singleton is absent. `pub(crate)` so `webtest` can reuse it for query-param reads and
/// the `window.__smash` refresh instead of duplicating the singleton dance.
#[cfg(target_arch = "wasm32")]
pub(crate) fn js_eval(code: &str) -> Variant {
    match js_bridge() {
        Some(mut js) => js.call("eval", &[code.to_variant(), true.to_variant()]),
        None => Variant::nil(),
    }
}

/// One `URLSearchParams` field off `location.search`, already percent-decoded by the browser (we
/// eval `URLSearchParams` rather than hand-split the raw string, so a `%XX`-escaped value round-
/// trips correctly). `None` when the param is absent, empty, or on native (no page to query).
/// Shared home for any boot-time query-param read -- `webtest`'s test-only `?autofind=`/`?script=`
/// AND product features like the `?join=` shareable-lobby link both need the same `js_eval` dance.
#[cfg(target_arch = "wasm32")]
pub(crate) fn query_param(name: &str) -> Option<String> {
    let code = format!(
        "(function(){{var v=new URLSearchParams(location.search).get('{name}');return v===null?'':v;}})()"
    );
    let s = js_eval(&code)
        .try_to::<GString>()
        .map(|g| g.to_string())
        .unwrap_or_default();
    (!s.is_empty()).then_some(s)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn query_param(_name: &str) -> Option<String> {
    None
}

/// The web page's `location.origin` ("https://host[:port]"), or `None` on native/headless. Lets the
/// client derive the relay host from wherever the game is actually served, instead of a baked URL.
pub(crate) fn page_origin() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let s = js_eval("location.origin")
            .try_to::<GString>()
            .map(|g| g.to_string())
            .unwrap_or_default();
        (!s.is_empty()).then_some(s)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// The active game's directory URL, preserving deployment paths such as /game3/.
pub(crate) fn page_directory() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let s = js_eval("new URL('.', location.href).href")
            .try_to::<GString>()
            .map(|g| g.to_string())
            .unwrap_or_default();
        (!s.is_empty()).then_some(s)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// The display's devicePixelRatio (logical->physical px). On web, gdext's
/// `DisplayServer::screen_get_scale()` returns 1.0 -- it never wires the browser's
/// `devicePixelRatio` in -- yet under `html/canvas_resize_policy=2` `Window::get_size()` IS physical
/// px (dpr-inflated). So `ui::scale::counter_scale_ppp` divides by a dpr-inflated stretch with dpr=1
/// and lands every point at 1/dpr logical size: the post-deploy "tiny UI" on a retina browser, the
/// same half-size mechanism `ui/scale.rs` already documents for native retina. Read
/// `window.devicePixelRatio` straight from JS instead; native falls back to the engine backing scale
/// (`screen_get_scale`), which is correct off the web.
pub(crate) fn device_pixel_ratio() -> f32 {
    #[cfg(target_arch = "wasm32")]
    {
        let dpr = js_eval("window.devicePixelRatio")
            .try_to::<f64>()
            .unwrap_or(0.0) as f32;
        if dpr > 0.0 {
            return dpr;
        }
    }
    godot::classes::DisplayServer::singleton().screen_get_scale()
}

/// Run the push subscribe flow (fires the browser permission prompt on first use).
#[cfg(target_arch = "wasm32")]
pub(crate) fn push_enable() {
    js_eval("window.smashPush && window.smashPush.enable && window.smashPush.enable()");
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn push_enable() {}

/// Current human push status (e.g. "pinging for 'default'"); empty when unset/unsupported/native.
#[cfg(target_arch = "wasm32")]
pub(crate) fn push_label() -> String {
    js_eval("(window.smashPush && window.smashPush.label) || ''")
        .try_to::<GString>()
        .map(|g| g.to_string())
        .unwrap_or_default()
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn push_label() -> String {
    String::new()
}

// --- Shareable quick-match invite -------------------------------------------------------------
// `KneeMan::invite` (kneeman/mod.rs) mints one of these, hosts it via `matchmake_room`, joins it
// with `rtc::invite_link` into a `?join=` URL, and copies that to the clipboard.

/// Alphabet for the invite code body: lowercase alphanumeric, so it reads clean typed into a URL
/// bar and speaks aloud fine.
const INVITE_CODE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Pure formatter: `seed` -> `"knee-"` + a 5-char body drawn from `INVITE_CODE_CHARS`. Split out of
/// `mint_invite_code` so the shape (prefix/charset/length) is unit-testable without Godot's clock.
fn format_invite_code(mut seed: u64) -> String {
    let mut body = String::with_capacity(5);
    for _ in 0..5 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        body.push(INVITE_CODE_CHARS[(seed >> 33) as usize % INVITE_CODE_CHARS.len()] as char);
    }
    format!("knee-{body}")
}

/// Mint a fresh invite room code, seeded off Godot's microsecond clock (same clock `mint_room_code`
/// uses). Shell-side randomness only -- this names a lobby, it never touches the sim.
pub(crate) fn mint_invite_code() -> String {
    format_invite_code(godot::classes::Time::singleton().get_ticks_usec())
}

/// Copy `text` to the OS/browser clipboard: web goes through the JS bridge
/// (`navigator.clipboard.writeText`), native uses Godot's `DisplayServer`. The web string is escaped
/// into a JS single-quoted literal -- invite links are an origin + a `knee-`-prefixed code (already
/// URL-safe), but escape defensively rather than trust it.
#[cfg(target_arch = "wasm32")]
pub(crate) fn copy_to_clipboard(text: &str) {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n");
    js_eval(&format!("navigator.clipboard.writeText('{escaped}')"));
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn copy_to_clipboard(text: &str) {
    godot::classes::DisplayServer::singleton().clipboard_set(text);
}

/// An empty lobby is culled after this long with nobody in it. The remaining countdown ships in each
/// lobby-list row so the grid can show it tick down.
pub const LOBBY_TTL_MS: u64 = 60_000;

/// One row in the versioned lobby browser. Shell-only view state (never in `SimState`/checksum). Today
/// `key` is the build version (lobbies are grouped/filtered by version — a "floating room by version");
/// the eventual generic lifecycle identity is `(key, host, created, active)`.
#[derive(Clone)]
pub struct LobbyRow {
    pub key: String,                 // version string today; any lifecycle key later
    pub host: String,                // user / creator
    pub active: u8,                  // players currently in
    pub cap: u8,                     // capacity
    pub empty_since_ms: Option<u64>, // Some once active == 0; drives the TTL countdown
}

impl LobbyRow {
    /// Seconds left before the relay culls this empty lobby, or `None` while it still has players.
    /// Saturates at 0 (a lobby past its TTL is on its way out this frame).
    pub fn ttl_remaining_secs(&self, now_ms: u64) -> Option<u32> {
        let since = self.empty_since_ms?;
        let elapsed = now_ms.saturating_sub(since);
        Some((LOBBY_TTL_MS.saturating_sub(elapsed) / 1000) as u32)
    }
}

/// Read-only snapshot of the netplay transport machine for the debug panel. Every field is a
/// `&'static str` (Copy) so it rides a `Mutable` cheaply; the human names are resolved in the shell
/// (here) where the godot WebRTC enums are in scope. Counts are (sent, received) for each handshake
/// frame kind so a stall shows where it stuck (offer out but no answer in = guest/relay drop, etc).
#[derive(Clone, Copy)]
pub struct NetDebug {
    pub phase: &'static str,
    pub role: &'static str,
    pub handle: usize,
    pub ws: &'static str,      // signaling socket
    pub conn: &'static str,    // RTCPeerConnection.connectionState
    pub gather: &'static str,  // ICE gathering
    pub signal: &'static str,  // SDP signaling
    pub channel: &'static str, // ggrs data channel
    pub offer: (u32, u32),
    pub answer: (u32, u32),
    pub ice: (u32, u32),
    // version/relay ping info for the Network page.
    pub build_hash: &'static str, // our own build hash (env!("BUILD_HASH"))
    pub stale_build: bool,        // relay reported a newer build -> reload warning
    pub peer_build_mismatch: bool, // peer SDP hash was received AND differs from ours
}

impl Default for NetDebug {
    fn default() -> Self {
        Self {
            phase: "offline",
            role: "—",
            handle: 0,
            ws: "—",
            conn: "—",
            gather: "—",
            signal: "—",
            channel: "—",
            offer: (0, 0),
            answer: (0, 0),
            ice: (0, 0),
            build_hash: crate::rtc::BUILD_HASH,
            stale_build: false,
            peer_build_mismatch: false,
        }
    }
}

// --- transport-state names for the debug panel. gdext models these as newtype structs (not real
// enums), so resolve by `==` rather than match patterns. ----------------------------------------
pub(crate) fn ws_name(s: WsState) -> &'static str {
    if s == WsState::CONNECTING {
        "connecting"
    } else if s == WsState::OPEN {
        "open"
    } else if s == WsState::CLOSING {
        "closing"
    } else {
        "closed"
    }
}
pub(crate) fn chan_name(s: ChannelState) -> &'static str {
    if s == ChannelState::CONNECTING {
        "connecting"
    } else if s == ChannelState::OPEN {
        "open"
    } else if s == ChannelState::CLOSING {
        "closing"
    } else {
        "closed"
    }
}
pub(crate) fn conn_name(s: ConnectionState) -> &'static str {
    if s == ConnectionState::NEW {
        "new"
    } else if s == ConnectionState::CONNECTING {
        "connecting"
    } else if s == ConnectionState::CONNECTED {
        "connected"
    } else if s == ConnectionState::DISCONNECTED {
        "disconnected"
    } else if s == ConnectionState::FAILED {
        "failed"
    } else {
        "closed"
    }
}
pub(crate) fn gather_name(s: GatheringState) -> &'static str {
    if s == GatheringState::NEW {
        "new"
    } else if s == GatheringState::GATHERING {
        "gathering"
    } else {
        "complete"
    }
}
pub(crate) fn signal_name(s: SignalingState) -> &'static str {
    if s == SignalingState::STABLE {
        "stable"
    } else if s == SignalingState::HAVE_LOCAL_OFFER {
        "have-local-offer"
    } else if s == SignalingState::HAVE_REMOTE_OFFER {
        "have-remote-offer"
    } else if s == SignalingState::HAVE_LOCAL_PRANSWER {
        "have-local-pranswer"
    } else if s == SignalingState::HAVE_REMOTE_PRANSWER {
        "have-remote-pranswer"
    } else {
        "closed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_code_has_the_knee_prefix_and_a_5char_body() {
        let code = format_invite_code(12345);
        assert!(code.starts_with("knee-"));
        let body = &code["knee-".len()..];
        assert_eq!(body.len(), 5);
        assert!(body.bytes().all(|b| INVITE_CODE_CHARS.contains(&b)));
    }

    #[test]
    fn invite_code_is_deterministic_per_seed() {
        assert_eq!(format_invite_code(42), format_invite_code(42));
    }

    #[test]
    fn invite_code_varies_across_seeds() {
        assert_ne!(format_invite_code(1), format_invite_code(2));
    }
}
