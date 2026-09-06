//! Browser-native netplay transport: Godot WebRTC instead of matchbox. matchbox is wasm-bindgen, so
//! it cannot compile into the emscripten web export; Godot's `WebRtcPeerConnection` maps straight to
//! the browser's RTCPeerConnection and ships in the web template. The ggrs core (`smash_net`) is
//! reused unchanged — this only supplies the socket + the signaling handshake. See AGENTS.md / the
//! GODOT_WEB.md Phase 2 notes.
//!
//! Topology: two browsers reach the signaling relay (`wss://.../rtc`), get paired host+guest, trade
//! SDP/ICE, then open ONE negotiated data channel (both sides create id=1, so no
//! `data_channel_received` plumbing). ggrs runs peer-to-peer over that channel; the relay sees no
//! gameplay. Handle order is fixed host=0 / guest=1 so both peers agree (see `crate::netplay::start_p2p`).

use godot::classes::web_rtc_data_channel::WriteMode;
use godot::classes::{Json, WebRtcDataChannel};
use godot::prelude::*;

use crate::netplay::{Message, NonBlockingSocket};

/// The relay origin, derived not baked: on the web export it is the page origin (whatever host served
/// the game -- serve from staging and it follows), on native it is `SMASH_RELAY` else a dev default.
/// Memoized (the web path is a JS `location.origin` eval), so per-frame callers pay it once.
pub fn relay_base() -> String {
    thread_local! {
        static BASE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }
    BASE.with(|b| {
        b.borrow_mut()
            .get_or_insert_with(|| {
                crate::net::page_origin()
                    .or_else(|| std::env::var("SMASH_RELAY").ok())
                    .unwrap_or_else(|| "https://hafley.codes".into())
            })
            .clone()
    })
}

/// Derive the ws(s) scheme from an http(s) origin: `https://` -> `wss://`, `http://` -> `ws://`.
/// Pure string transform, factored out of `signaling_url` so it's unit-testable without dragging in
/// `relay_base`'s thread-local/JS-eval plumbing. Matters for local/E2E testing: `location.origin` on
/// a plain `http://localhost` dev server must NOT come out `wss://` (that origin has no TLS to
/// upgrade to, so a `wss://` dial would just fail) -- the https branch is tried first so an
/// already-http origin can't accidentally match it.
fn ws_scheme(origin: &str) -> String {
    origin
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}

/// WebSocket signaling endpoint (`/rtc`); scheme follows the origin (https->wss, http->ws). The relay
/// pairs two dialers and forwards their SDP/ICE.
pub fn signaling_url() -> String {
    format!("{}/rtc", ws_scheme(&relay_base()))
}

/// Build the shareable `?join=` invite link: `base` (the serving game directory URL) joined
/// with `code` (a minted invite room code, see `net::mint_invite_code`). Pure string join, split out
/// so it's unit-testable without `relay_base`'s thread-local/JS-eval plumbing (same reasoning as
/// `ws_scheme`).
pub fn invite_link(base: &str, code: &str) -> String {
    format!(
        "{}?join={code}",
        base.trim_end_matches('/').to_owned() + "/"
    )
}

/// The relay's plain-HTTP status/JSON page (same host+route as signaling; it answers JSON without the
/// WebSocket upgrade header). The debug panel fetches this.
pub fn status_url() -> String {
    format!("{}/rtc", relay_base())
}

/// Netcode event firehose sink (POST). nginx forwards this to the signaling binary's `/ev`, which
/// stamps client IP + recv time and appends to a rotating JSON-lines log. See `analytics`.
pub fn event_url() -> String {
    format!("{}/ev", relay_base())
}

/// Source fingerprint of both workspaces, stamped by build.rs. Sent in the dial URL and
/// SDP handshake. Includes dirty sources; excludes checkout paths and Git metadata.
/// A relay advertising an older Git-hash stamp cannot certify freshness of this source stamp.
pub const BUILD_HASH: &str = env!("BUILD_HASH");

/// Frames of input delay fed to ggrs. Higher = fewer rollbacks but more felt latency. 2 is a sane
/// LAN/decent-connection default.
pub const INPUT_DELAY: usize = 2;

/// Which side of the pair this peer is. The relay assigns it (first dialer = host). Fixes the ggrs
/// handle: host is player 0, guest is player 1.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Role {
    Host,
    Guest,
}

impl Role {
    pub fn from_str(s: &str) -> Option<Role> {
        match s {
            "host" => Some(Role::Host),
            "guest" => Some(Role::Guest),
            _ => None,
        }
    }
    /// (local ggrs handle, the remote's address as this peer's socket tags it).
    pub fn handles(self) -> (usize, usize) {
        match self {
            Role::Host => (0, 1),
            Role::Guest => (1, 0),
        }
    }
}

/// ggrs `NonBlockingSocket` over one Godot data channel. There is exactly one remote, so the address
/// is trivial: every inbound packet is tagged with `remote`, and `send_to` ignores its address arg
/// (only one place to send). bincode wire, same as the matchbox impl.
// reuse-kit-library(godot-rtc-transport): shared ggrs NonBlockingSocket over a Godot WebRtcDataChannel + ice_config()/TURN credential contract, consumed by both V1 (kneeman/session) and V4 (v4_net::GodotPair).
pub struct RtcSocket {
    pub channel: Gd<WebRtcDataChannel>,
    pub remote: usize,
}

impl NonBlockingSocket<usize> for RtcSocket {
    fn send_to(&mut self, msg: &Message, _addr: &usize) {
        let bytes = bincode::serialize(msg).expect("serialize ggrs message");
        let packet = PackedByteArray::from(bytes.as_slice());
        self.channel.put_packet(&packet);
    }

    fn receive_all_messages(&mut self) -> Vec<(usize, Message)> {
        let mut out = Vec::new();
        let n = self.channel.get_available_packet_count();
        for _ in 0..n {
            let packet = self.channel.get_packet();
            if let Ok(msg) = bincode::deserialize::<Message>(packet.as_slice()) {
                out.push((self.remote, msg));
            }
        }
        out
    }
}

/// Build the negotiated data-channel options dict: id 1, both sides create it (no signaling of the
/// channel itself), unreliable + unordered (ggrs has its own reliability layer).
pub fn data_channel_options() -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("negotiated", true);
    d.set("id", 1);
    d.set("ordered", false);
    d.set("maxRetransmits", 0);
    d
}

/// TURN credential endpoint (GET). The relay mints a short-lived HMAC credential for the coturn relay
/// (see signaling/src/turn.rs); prefetched at boot into `TURN_CREDS`. Absent host/secret => 404 => we
/// stay STUN-only.
pub fn turn_url() -> String {
    format!("{}/turn", relay_base())
}

/// Ephemeral coturn REST credential, as returned by `/turn`. Cached process-wide after the boot fetch;
/// `ice_config` folds it into the ICE server list so ICE can relay when a direct path fails.
#[derive(Clone, Default)]
pub struct TurnCreds {
    pub urls: Vec<String>, // e.g. ["turn:hafley.codes:3478?transport=udp", "...tcp"]
    pub username: String,  // unix-expiry string
    pub credential: String, // base64(HMAC-SHA1(secret, username))
}

thread_local! {
    static TURN_CREDS: std::cell::RefCell<Option<TurnCreds>> = const { std::cell::RefCell::new(None) };
}

/// Parse a `/turn` JSON response and cache it. No-op on malformed/empty bodies (stays STUN-only).
pub fn store_turn_creds(text: &GString) {
    let d = parse_json(text);
    let username = dget_str(&d, "username");
    let credential = dget_str(&d, "credential");
    let urls: Vec<String> = d
        .get("urls")
        .and_then(|v| v.try_to::<VarArray>().ok())
        .map(|a| {
            a.iter_shared()
                .filter_map(|v| v.try_to::<GString>().ok())
                .map(|g| g.to_string())
                .collect()
        })
        .unwrap_or_default();
    if username.is_empty() || credential.is_empty() || urls.is_empty() {
        return;
    }
    TURN_CREDS.with(|c| {
        *c.borrow_mut() = Some(TurnCreds {
            urls,
            username,
            credential,
        })
    });
}

/// Count of cached TURN urls (0 = STUN-only). For the firehose `turn` event.
pub fn turn_url_count() -> usize {
    TURN_CREDS.with(|c| c.borrow().as_ref().map(|t| t.urls.len()).unwrap_or(0))
}

/// ICE config dict for `WebRtcPeerConnection::initialize`. Always offers a public STUN server (enough
/// for most home NATs); when a TURN credential has been fetched (`store_turn_creds`), it is appended
/// as a relay fallback for symmetric-NAT / VPN peer pairs. ICE prefers direct and only relays on need.
pub fn ice_config() -> VarDictionary {
    let mut stun = VarDictionary::new();
    stun.set("urls", "stun:stun.l.google.com:19302");
    let mut servers = varray![stun];
    // One server entry per url STRING (not an array): Godot's web WebRTC only honors a string `urls`,
    // matching the working STUN entry above. An array-valued `urls` is silently dropped by the browser
    // bridge, so the relay candidate never gathers.
    TURN_CREDS.with(|c| {
        if let Some(t) = c.borrow().as_ref() {
            for u in &t.urls {
                let mut turn = VarDictionary::new();
                turn.set("urls", u.as_str());
                turn.set("username", t.username.as_str());
                turn.set("credential", t.credential.as_str());
                servers.push(&turn.to_variant());
            }
        }
    });
    let mut cfg = VarDictionary::new();
    cfg.set("iceServers", servers);
    cfg
}

/// Make a channel binary-mode (ggrs ships raw bytes, not strings).
pub fn set_binary(channel: &mut Gd<WebRtcDataChannel>) {
    channel.set_write_mode(WriteMode::BINARY);
}

// --- tiny JSON helpers over Godot's Json (handles SDP newlines/escaping for us) ---------------

/// Serialize a `{kind: ...}` signaling message to a JSON string for the WebSocket.
pub fn to_json(d: &VarDictionary) -> GString {
    Json::stringify(&d.to_variant())
}

/// Parse an inbound signaling frame into a VarDictionary (empty on malformed input).
pub fn parse_json(text: &GString) -> VarDictionary {
    let v = Json::parse_string(text);
    v.try_to::<VarDictionary>().unwrap_or_default()
}

/// Read a string field, defaulting to "" so callers can match on it directly.
pub fn dget_str(d: &VarDictionary, key: &str) -> String {
    d.get(key)
        .and_then(|v| v.try_to::<GString>().ok())
        .map(|g| g.to_string())
        .unwrap_or_default()
}

/// Variant -> i64, accepting BOTH int and float variants. Godot's `Json.parse_string` types every
/// JSON number as FLOAT (JSON doesn't distinguish), so a strict `try_to::<i64>` on anything that
/// round-tripped through the relay always fails to the default -- which is how the handshake's
/// `pchar` silently never arrived (the frame-0 char_id desync the E2E parity canary caught), and
/// why the ICE `index` only worked because its default 0 matches the single media line.
fn variant_int(v: &Variant) -> Option<i64> {
    v.try_to::<i64>()
        .ok()
        .or_else(|| v.try_to::<f64>().ok().map(|f| f as i64))
}

/// Read an int field (ICE candidate index), defaulting to 0.
pub fn dget_int(d: &VarDictionary, key: &str) -> i64 {
    d.get(key).as_ref().and_then(variant_int).unwrap_or(0)
}

/// Read an int field with a caller-chosen default for absent/mistyped keys (e.g. -1 to mark "a peer
/// on an older build didn't send this field").
pub fn dget_int_or(d: &VarDictionary, key: &str, default: i64) -> i64 {
    d.get(key).as_ref().and_then(variant_int).unwrap_or(default)
}

// --- WIRE PROTOCOL v2 (mesh netplay): pure parse/decision helpers -------------------------------
// Kept here (not in `kneeman/mesh.rs`, which owns the Godot-typed orchestration) so they're plain
// `cargo test`-able without a live engine: no `Gd<>`/`VarDictionary` in their signatures, only the
// already-extracted primitives. `kneeman/mod.rs` and `kneeman/session.rs` are at their
// `.dl/lint-file-budget.dl` ratchet caps; this file (rtc.rs) has the room.

/// Resolve this peer's `(handle, party size)` from a parsed "matched" frame's optional `handle`/
/// `count` fields (pass `None` when the relay omitted them, e.g. via `dget_int_or(d, key, -1)` then
/// filtering `< 0` to `None` at the call site). An old (pre-mesh) relay never sends them at all —
/// that's a party of 2, and the handle is exactly what `role.handles().0` already gave: this
/// reproduces that byte-for-byte instead of introducing a second source of truth for it.
pub fn resolve_matched(handle: Option<i64>, count: Option<i64>, role: Role) -> (usize, usize) {
    let fallback_handle = role.handles().0;
    let h = handle
        .filter(|&h| h >= 0)
        .map(|h| h as usize)
        .unwrap_or(fallback_handle);
    let k = count.filter(|&c| c > 0).map(|c| c as usize).unwrap_or(2);
    (h, k)
}

/// Mid-match reconnect re-pair (k==2): the relay assigns handle 0 to whichever peer's socket
/// re-dialed the private room FIRST (`join_party` is raw join order), so a transport blip could
/// SWAP the pair's handles -- and with the resume snapshot keeping every fighter in its old slot,
/// each player came back driving the OTHER character. Keep the pre-drop handle instead; both
/// peers apply this, so the offer/answer + resume/tune duties stay a consistent host/guest pair.
/// A fresh match (or the k>2 path, which never reconnects -- it aborts) takes the relay's word.
pub fn resolve_rematched(
    reconnecting: bool,
    count: usize,
    prev_handle: usize,
    fresh: (usize, Role),
) -> (usize, Role) {
    if reconnecting && count <= 2 {
        let role = if prev_handle == 0 {
            Role::Host
        } else {
            Role::Guest
        };
        (prev_handle, role)
    } else {
        fresh
    }
}

/// Pair initiator rule (WIRE PROTOCOL v2 #3): for the unordered pair `(local, peer)`, the LOWER
/// handle creates the offer; the higher one waits for it. Callers never invoke this with
/// `local == peer` (no self-connection), so a tie can't arise in practice.
pub fn is_initiator(local: usize, peer: usize) -> bool {
    local < peer
}

/// Resolve the peer handle an inbound offer/answer/ice frame concerns from its optional "to"/"from"
/// field. `fallback` is "the other one" (`role.handles().1`) — the ONLY valid reading of a frame
/// that omits the field, which only ever happens on a party of 2 talking to a not-yet-upgraded
/// relay (WIRE PROTOCOL v2 #2).
pub fn resolve_peer_field(field: Option<i64>, fallback: usize) -> usize {
    field
        .filter(|&h| h >= 0)
        .map(|h| h as usize)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_becomes_wss() {
        assert_eq!(ws_scheme("https://hafley.codes"), "wss://hafley.codes");
    }

    #[test]
    fn http_becomes_ws() {
        // The E2E/local-dev case: an http:// origin must dial ws://, never wss:// (there is no TLS
        // to upgrade to on plain localhost).
        assert_eq!(ws_scheme("http://localhost:8080"), "ws://localhost:8080");
    }

    #[test]
    fn scheme_replaced_once_each_no_cascade() {
        // Pathological input shouldn't cascade a second replacement through the first one's output.
        assert_eq!(ws_scheme("http://http://nested"), "ws://http://nested");
        assert_eq!(ws_scheme("https://https://nested"), "wss://https://nested");
    }

    #[test]
    fn scheme_missing_passes_through() {
        assert_eq!(ws_scheme("localhost:8080"), "localhost:8080");
    }

    #[test]
    fn invite_link_preserves_game_directory() {
        for (base, expected) in [
            (
                "https://hafley.codes/game/",
                "https://hafley.codes/game/?join=knee-a1b2c",
            ),
            (
                "https://hafley.codes/game3/",
                "https://hafley.codes/game3/?join=knee-a1b2c",
            ),
            (
                "http://127.0.0.1:8787/game3",
                "http://127.0.0.1:8787/game3/?join=knee-a1b2c",
            ),
        ] {
            assert_eq!(invite_link(base, "knee-a1b2c"), expected);
        }
    }

    // --- WIRE PROTOCOL v2 pure helpers ---------------------------------------------------------

    #[test]
    fn resolve_matched_prefers_explicit_handle_and_count() {
        assert_eq!(resolve_matched(Some(2), Some(4), Role::Guest), (2, 4));
    }

    #[test]
    fn resolve_matched_falls_back_to_role_and_a_party_of_two() {
        // Old relay: no handle/count fields at all -- role alone decides, party of 2.
        assert_eq!(resolve_matched(None, None, Role::Host), (0, 2));
        assert_eq!(resolve_matched(None, None, Role::Guest), (1, 2));
    }

    #[test]
    fn resolve_matched_negative_sentinel_also_falls_back() {
        // Call sites read absent fields via `dget_int_or(d, key, -1)`; -1 must mean "absent" too.
        assert_eq!(resolve_matched(Some(-1), Some(-1), Role::Guest), (1, 2));
    }

    #[test]
    fn rematch_keeps_the_predrop_handle_whatever_redial_order_said() {
        // The mid-match player swap: old guest (handle 1) re-dialed first, relay crowned it
        // host/0 -- the reconnect override hands its old identity back. Both sides of the swap:
        assert_eq!(
            resolve_rematched(true, 2, 1, (0, Role::Host)),
            (1, Role::Guest)
        );
        assert_eq!(
            resolve_rematched(true, 2, 0, (1, Role::Guest)),
            (0, Role::Host)
        );
        // ...and a lucky re-dial order that already matches is a no-op.
        assert_eq!(
            resolve_rematched(true, 2, 0, (0, Role::Host)),
            (0, Role::Host)
        );
    }

    #[test]
    fn fresh_match_takes_the_relays_assignment() {
        // Not reconnecting: the stale prev_handle (whatever a past session left behind) is ignored.
        assert_eq!(
            resolve_rematched(false, 2, 1, (0, Role::Host)),
            (0, Role::Host)
        );
        // k>2 never reconnects (abort_party punts), so even a Reconnecting flag defers to the relay.
        assert_eq!(
            resolve_rematched(true, 4, 1, (2, Role::Guest)),
            (2, Role::Guest)
        );
    }

    #[test]
    fn initiator_is_the_lower_handle() {
        assert!(is_initiator(0, 1));
        assert!(is_initiator(1, 3));
        assert!(!is_initiator(2, 1));
        assert!(!is_initiator(1, 1)); // never invoked this way, but stays false, not true
    }

    #[test]
    fn resolve_peer_field_prefers_the_explicit_handle() {
        assert_eq!(resolve_peer_field(Some(3), 1), 3);
    }

    #[test]
    fn resolve_peer_field_falls_back_to_the_other_one_when_absent() {
        assert_eq!(resolve_peer_field(None, 1), 1);
        assert_eq!(resolve_peer_field(Some(-1), 0), 0);
    }
}
