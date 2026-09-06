use futures_signals::signal::Mutable;
use godot::classes::{
    AnimatedSprite2D, Button, Camera2D, CanvasLayer, ColorRect, Gradient, GradientTexture2D,
    HttpRequest, INode2D, InputEvent, Label, Node2D, Panel, Polygon2D, Texture2D, TextureRect,
    ThemeDb, WebRtcDataChannel, WebRtcPeerConnection, WebSocketPeer,
};
use godot::prelude::*;
use godot::tools::try_load;

use crate::identity::{Identity, load_identity, slot_color, slot_name};
use crate::net::{NetDebug, now_ms};
use crate::roster::roster;
use crate::rtc::{self, Role};
use crate::sim::net::{NetInput, Netplay};
use crate::sim::{self, CharState, SimState, Tune};
use crate::sprite::{apply_character, make_edge_tag, make_hud_label, make_tag};

/// Optional texture override for the Armored Core mech body (`draw_mech`). `repo-root/assets/` is
/// mounted as `res://assets/` (see `roster::load_roster_json`'s use of the same prefix). Loaded
/// once in `ready()` via `try_load`, which yields `None` for a missing file instead of erroring.
/// The kit-model source art FACES LEFT, so the draw mirrors it when the fighter faces right.
const MECH_TEX: &str = "res://assets/armored-core-kit-model.jpg";
/// Fit scale for `MECH_TEX` over the painted mech's head-to-feet span (1.0 == same height as the
/// painted fallback); bump/drop this to hand-tune the PNG's on-screen size.
const MECH_SCALE: f32 = 1.15;

/// Z-index for the KneeMan node's OWN `_draw()` pass (gizmos/items/fx/tooltip/etc, all painted via
/// `self.base_mut().draw_*`), sat between the fighter sprites (implicit 0) and the nametag/HUD
/// labels (`sprite.rs` uses 100, "above the sprites"). Godot draws a parent's own content BEFORE its
/// children/later siblings when z is tied, so at z=0 this node's whole draw() -- including the
/// pickup tooltip, last in the sequence -- rendered UNDER both the local player's own sprite (a
/// child, "Anim") and the other slot's sprite (a sibling added after this node). Bumping this node's
/// z_index above 0 fixes the sibling case outright; slot 0's "Anim" child inherits the same offset
/// (z_as_relative defaults true) and needs the compensating `-OVERLAY_Z` below to land back at its
/// original effective 0 (unchanged order vs the stage background), while now sitting strictly BELOW
/// this node's own content.
const OVERLAY_Z: i32 = 60;

/// Netplay lifecycle. Offline = local single-player (default). Signaling = dialing the relay +
/// doing the WebRTC handshake; still renders local play so the page isn't frozen. Running = ggrs
/// rollback drives the sim from both peers' inputs. Reconnecting = the peer dropped mid-match; we
/// re-dial the private room and hold a window for them to come back before falling to Offline.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Phase {
    Offline,
    Signaling,
    Running,
    Reconnecting,
}

/// How long (ms) to keep a match's room alive after a transport drop, waiting for the dropped peer
/// to re-dial and re-pair. Past this, the room is freed and we return to local play ("turn off").
const RECONNECT_WINDOW_MS: u64 = 12_000;

/// The match's room identity on the client (the "lobby entity"). `code` is the private room both
/// peers re-dial to find EACH OTHER again after a transport drop — the host mints it once paired and
/// ships it to the guest over the signaling socket; the relay forwards it verbatim, so no server
/// change is needed. `deadline_ms` is `Some` only while we're inside the reconnect window.
struct Room {
    code: String,
    deadline_ms: Option<u64>,
}

/// Signaling-frame tallies kept on the node; folded into `NetDebug` each frame.
#[derive(Default, Clone, Copy)]
struct SigCounts {
    offer_out: u32,
    offer_in: u32,
    answer_out: u32,
    answer_in: u32,
    ice_out: u32,
    ice_in: u32,
}

/// Boundary: the pure sim speaks glam::Vec2; Godot wants its own Vector2. Convert on the way out.
#[inline]
pub(crate) fn gv(v: sim::Vector2) -> Vector2 {
    Vector2::new(v.x, v.y)
}

/// Deterministic pseudo-random 0..1 float from an fx's spawn tick + a per-use salt (spike index,
/// etc). Cosmetic jitter only — never touches the sim's own RNG — so an explosion's spikes look
/// scraggly without needing any state beyond the fx itself.
fn fx_jitter(tick: u64, salt: u64) -> f32 {
    let h = tick
        .wrapping_mul(2654435761)
        .wrapping_add(salt.wrapping_mul(40503));
    ((h % 1000) as f32) / 1000.0
}

#[path = "0_debugger.rs"]
mod debugger;
mod mesh;
mod persist;
mod render;
mod session;
mod touch;

use touch::{
    QUADS, TOUCH_BTNS, TOUCH_CSTICK, TOUCH_CSTICK_FINGER, TOUCH_CSTICK_ORIGIN, TOUCH_CSTICK_RAD,
    TOUCH_DIAMOND, TOUCH_FINGER, TOUCH_ORIGIN, TOUCH_STICK, TOUCH_STICK_RAD, TOUCH_STICK_ZONE_X,
    quad_at,
};

/// Owns the BehaviorSubjects (state + tune). Each tick: sample input -> pure step ->
/// publish into the state cell -> render (position + sprite clip). That is the only
/// place effects live; `sim::step` itself is pure.
#[derive(GodotClass)]
#[class(base = Node2D)]
pub struct KneeMan {
    debugger: debugger::Debugger,
    base: Base<Node2D>,
    state: Mutable<SimState>, // source of truth, observed everywhere
    tune: Mutable<Tune>,      // live config, written by egui
    gizmos: Mutable<bool>, // debug overlay toggle: draw ECB/hurtbox/hitbox boxes (panel checkbox)
    // Per-fighter render slots, indexed 0..active. `sprites[0]` is the node's own "Anim" child
    // (positioned via the node); `sprites[1..]` are world-space siblings positioned each frame.
    sprites: [Option<Gd<AnimatedSprite2D>>; sim::MAX_PLAYERS], // driven by CharState per fighter
    dummy: Option<Gd<ColorRect>>, // legacy P2 block (hidden once the P2 sprite exists)
    mech_tex: Option<Gd<Texture2D>>, // cached AC mech-body texture (MECH_TEX); None paints today's boxy body
    tags: [Option<Gd<Label>>; sim::MAX_PLAYERS], // world-space nametags hovering over each head
    edge_tags: [Option<Gd<Label>>; sim::MAX_PLAYERS], // screen-space "off-stage" chips: name+arrow+dist
    prev_pos: [sim::Vector2; sim::MAX_PLAYERS], // last frame's feet pos, for KO teleport detection (bangs)
    trails: [Vec<sim::Vector2>; sim::MAX_PLAYERS], // recent feet positions per fighter, for the fast-move smear
    bangs: Vec<(Vector2, f32)>, // active blast flashes: world pos + age (0..1), drawn in draw()
    puffs: Vec<(Vector2, f32)>, // dirt cloud puffs: world pos + age (0..1); hard-brake / landing skid
    flashes: Vec<(Vector2, f32)>, // special-attack flashes: world pos + age (0..1); B-move startup
    prev_state: [CharState; sim::MAX_PLAYERS], // prior-frame CharState, for transition edge detection
    prev_vel: [sim::Vector2; sim::MAX_PLAYERS], // prior-frame velocity, for landing-skid speed check
    last_aim: sim::Vector2, // local c-stick this frame, drives the local aim-arrow tell (cosmetic)
    spawn_prev: [bool; 10], // last-frame press state of number keys 1..0, for rising-edge test-spawns
    pressure_frames: u32,   // consecutive frames with paths/items occupancy over the hot line
    pressure_warned: bool,  // one warn toast per hot episode; re-arms when occupancy drops
    status: Option<Gd<Button>>, // screen-space netplay status chip; tap it to find a match
    hud: [Option<Gd<Label>>; sim::MAX_PLAYERS], // bottom damage panel: each fighter's name + %
    cam: Option<Gd<Camera2D>>, // sibling Camera2D, tracked to fit both fighters each frame
    stick_base: Option<Gd<Panel>>, // touch stick ring (follows the active finger origin)
    stick_knob: Option<Gd<Panel>>, // touch stick knob (offset by the current tilt)
    cstick_base: Option<Gd<Panel>>, // touch c-stick ring (yellow; parked by the diamond until grabbed)
    cstick_knob: Option<Gd<Panel>>, // touch c-stick nub (offset by the current tilt)
    quad_polys: Vec<Gd<Polygon2D>>, // 4 diamond wedge fills (indexed by Dir: Top,Left,Right,Bottom)
    quad_labels: Vec<Gd<Label>>,    // wedge letter labels (same order)
    shorthop_panel: Option<Gd<Panel>>, // rectangle under the bottom tip
    shorthop_label: Option<Gd<Label>>,
    menu_btn: Option<Gd<Button>>, // bottom-center MENU tab: opens the XP pause menu
    debug_ui: Option<Gd<crate::ui::debug::DebugUi>>, // sibling egui panel, toggled by the MENU button
    netdbg: Mutable<NetDebug>,                       // transport snapshot, read by the debug panel
    sig: SigCounts,                                  // handshake-frame tallies feeding netdbg
    identity: Mutable<Identity>, // local player name+color, edited by the panel, persisted
    saved_identity: Identity,    // last value written to localStorage (change detection)
    charsel: Mutable<[i64; 2]>,  // P1/P2 roster pick, written by the menu, applied live
    saved_charsel: [i64; 2],     // last picks written to storage (change detection)
    toasts: crate::toast::Toasts, // global snackbar queue, emitted on phase changes, drawn by DebugUi
    world: Option<crate::world_runtime::WorldRuntime<crate::godot_store::GodotStore>>, // durable home world (user://)
    ink_base: std::collections::BTreeMap<Vec<u8>, sim::world::EventId>, // persisted-ink diff base: stroke content key -> placing event
    characters: [usize; sim::MAX_PLAYERS], // per-fighter index into ROSTER; charsel drives slots 0..2
    base_scale: [f32; sim::MAX_PLAYERS],   // each sprite's resting scale (impact-pop multiplies it)

    // --- netplay (Godot WebRTC). All None/Offline until the player taps the status chip to join. ---
    phase: Phase,
    role: Option<Role>,
    local_handle: usize,           // ggrs handle for this peer (host 0 / guest 1)
    ws: Option<Gd<WebSocketPeer>>, // signaling socket to the relay
    pc: Option<Gd<WebRtcPeerConnection>>, // the P2P connection
    channel: Option<Gd<WebRtcDataChannel>>, // negotiated data channel ggrs rides
    net: Option<Box<dyn Netplay<State = SimState, Input = NetInput>>>, // model-agnostic session seam (rollback today)
    room: Option<Room>, // match's room identity; survives a drop so we can rejoin
    resume_snapshot: Option<SimState>, // sim state captured at a drop, to resume the rebuilt session from
    got_resume: bool, // pair: accepted the remote SDP startup selection (including fresh starts)
    got_tune: bool, // pair startup or mesh host supplied the authoritative Tune
    party_count: usize, // k: this match's player count (matched.count; back-compat default 2)
    mesh: Option<Box<mesh::MeshSession>>, // k>2 per-peer state; None on the untouched k<=2 path

    // --- version compatibility (build-hash ping; see rtc::BUILD_HASH) ---
    http: Option<Gd<HttpRequest>>, // refetches the relay /status on refocus to spot a stale build
    peer_build: Option<String>, // opponent's build hash from the SDP handshake (None until traded)
    stale_build: bool,          // our wasm is older than the live server build -> reload
    peer_identity: Option<Identity>, // remote player's chosen name+color from the SDP handshake
    peer_char: Option<usize>, // remote player's roster pick from the handshake (display-only, per-peer)

    // --- netcode event firehose (see `analytics`); its own HttpRequest so it never collides with the
    // /status fetch above. `last_net` edge-detects transport changes so we log deltas, not every frame.
    analytics_http: Option<Gd<HttpRequest>>,
    ev_tick: u32,       // frame counter: flush the buffer + heartbeat on a cadence
    last_net: NetDebug, // previous transport snapshot, for change detection

    // --- TURN: prefetch coturn REST creds at boot so ice_config() can add a relay fallback (rtc.rs) ---
    turn_http: Option<Gd<HttpRequest>>, // GET /turn once at boot; result cached via rtc::store_turn_creds
    ice_typ_seen: u8, // bitmask of ICE candidate types logged this session (host/srflx/relay/prflx)
}

#[godot_api]
impl INode2D for KneeMan {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            debugger: debugger::Debugger::default(),
            base,
            state: Mutable::new(session::local_spawn()),
            tune: Mutable::new(Tune::default()),
            gizmos: Mutable::new(true), // hitbox/hurtbox/ECB overlay ON by default
            sprites: Default::default(),
            dummy: None,
            mech_tex: None,
            tags: Default::default(),
            edge_tags: Default::default(),
            prev_pos: [sim::Vector2::new(0.0, 0.0); sim::MAX_PLAYERS],
            trails: Default::default(),
            bangs: Vec::new(),
            puffs: Vec::new(),
            flashes: Vec::new(),
            prev_state: [CharState::Stand; sim::MAX_PLAYERS],
            prev_vel: [sim::Vector2::new(0.0, 0.0); sim::MAX_PLAYERS],
            last_aim: sim::Vector2::new(0.0, 0.0),
            spawn_prev: [false; 10],
            pressure_frames: 0,
            pressure_warned: false,
            status: None,
            hud: Default::default(),
            cam: None,
            stick_base: None,
            stick_knob: None,
            cstick_base: None,
            cstick_knob: None,
            quad_polys: Vec::new(),
            quad_labels: Vec::new(),
            shorthop_panel: None,
            shorthop_label: None,
            menu_btn: None,
            debug_ui: None,
            netdbg: Mutable::new(NetDebug::default()),
            analytics_http: None,
            ev_tick: 0,
            last_net: NetDebug::default(),
            turn_http: None,
            ice_typ_seen: 0,
            sig: SigCounts::default(),
            identity: Mutable::new(Identity::default()),
            saved_identity: Identity::default(),
            charsel: Mutable::new([0, 1]),
            saved_charsel: [0, 1],
            toasts: Mutable::new(Vec::new()),
            world: None,
            ink_base: std::collections::BTreeMap::new(),
            characters: [0, 1, 0, 1], // default: frog/zombie alternating; charsel overrides slots 0..2
            base_scale: [1.0; sim::MAX_PLAYERS],
            phase: Phase::Offline,
            role: None,
            local_handle: 0,
            ws: None,
            pc: None,
            channel: None,
            net: None,
            room: None,
            resume_snapshot: None,
            got_resume: false,
            got_tune: false,
            party_count: 2,
            mesh: None,
            http: None,
            peer_build: None,
            stale_build: false,
            peer_identity: None,
            peer_char: None,
        }
    }

    fn ready(&mut self) {
        crate::controls::bindings::load();
        let pos = self.state.get().fighters[0].pos;
        self.base_mut().set_position(gv(pos));
        self.base_mut().set_z_index(OVERLAY_Z); // see OVERLAY_Z: sits above every fighter sprite

        // Load the saved identity (web) before building tags so slot 0's tag wears the right name/color.
        let id = load_identity();
        self.identity.set(id.clone());
        self.saved_identity = id.clone();

        // Saved roster picks ride the same file; sync_charsel clamps + applies them on the
        // first frame, so the character you last played comes back with the name and color.
        let sel = crate::identity::load_charsel();
        self.charsel.set(sel);
        self.saved_charsel = sel;

        // Boot the durable home world (user:// via GodotStore). Owner key is persisted + stable, so the
        // same home re-attaches every launch. This is load/save; nothing renders it yet.
        let owner = crate::godot_store::load_or_make_owner();
        let rt =
            crate::world_runtime::WorldRuntime::boot(crate::godot_store::GodotStore::open(), owner);
        let w = rt.world();
        godot_print!(
            "world: home {}… loaded, {} platforms, {} strokes, bg={}",
            &crate::godot_store::hex32(&owner.0)[..8],
            w.platforms.len(),
            w.strokes.len(),
            w.bg.is_some()
        );
        // parity(v1-ink-durable-world-rehydrate): boot rebuilds persisted stroke facts into live path slots and seeds the content-keyed diff base so unchanged ink is not appended again
        // Rehydrate persisted permanent ink into live sim paths (a joiner sees the chaos immediately).
        // Seed the diff base with the same content keys `persist_ink` computes, so boot doesn't
        // re-append what's already in the log.
        {
            let mut s = self.state.get();
            let t = self.tune.get_cloned();
            let mut slot = 0usize;
            for (id, st) in w.strokes.iter() {
                while slot < sim::MAX_DRAWN && s.paths[slot].active() {
                    slot += 1;
                }
                if slot >= sim::MAX_DRAWN {
                    break; // more persisted strokes than slots: the rest stay in the log only
                }
                let path =
                    sim::rehydrate_stroke(&st.pts, st.stroke, 0, &mut s.nodes, &mut s.free, &t);
                s.paths[slot] = path;
                self.ink_base
                    .insert(sim::world::canon(&(st.stroke, st.pts.clone())), *id);
            }
            self.state.set(s);
        }
        self.world = Some(rt);

        // Armored Core mech-body art, loaded once (not per-draw): a missing file is the expected
        // default today, so `try_load` -> `.ok()` yields None quietly rather than a warning spam.
        self.mech_tex = try_load::<Texture2D>(MECH_TEX).ok();

        // Legacy P2 block: hide it, the per-fighter sprites replace it.
        self.dummy = self
            .base()
            .get_node_or_null("../Dummy")
            .and_then(|n| n.try_cast::<ColorRect>().ok());
        if let Some(d) = self.dummy.as_mut() {
            d.set_visible(false);
        }

        // Per-fighter sprites + nametags, one slot per possible player. Slot 0 is the node's own
        // "Anim" child (positioned via the node itself); slots 1.. are world-space siblings under the
        // parent, positioned each frame in `render_fighter`. Tags are world-space labels hovering over
        // each head, wearing the slot color. Built here, not in a .tres, so the CharState->clip wiring
        // stays readable. Dormant slots (>= active) are hidden each frame by the render loop.
        let roster = roster();
        for k in 0..sim::MAX_PLAYERS {
            let c = &roster[self.characters[k].min(roster.len() - 1)];
            let color = if k == 0 { id.color } else { slot_color(k) };
            let name = if k == 0 {
                id.name.clone()
            } else {
                slot_name(k)
            };
            let tag = make_tag(&name, color, id.font_px);

            let sprite = if k == 0 {
                // Slot 0: the authored "Anim" child; it tracks the node, so no per-frame position.
                self.base()
                    .get_node_or_null("Anim")
                    .and_then(|n| n.try_cast::<AnimatedSprite2D>().ok())
            } else {
                Some(AnimatedSprite2D::new_alloc())
            };
            if let Some(mut a) = sprite {
                apply_character(&mut a, c);
                self.base_scale[k] = c.scale;
                if k == 0 {
                    // Anim is this node's own child, so it inherits OVERLAY_Z (z_as_relative):
                    // cancel that offset so slot 0 keeps its original effective z (0), same order
                    // vs the stage/background as before, but now strictly under this node's draw().
                    a.set_z_index(-OVERLAY_Z);
                }
                // World-space siblings (slots 1..) add deferred: during ready() the parent is still
                // "busy setting up children", so an immediate add_child is rejected. The tag is a
                // world-space sibling for every slot.
                if let Some(mut parent) = self.base().get_parent() {
                    if k != 0 {
                        parent.call_deferred("add_child", &[a.to_variant()]);
                    }
                    parent.call_deferred("add_child", &[tag.to_variant()]);
                }
                self.sprites[k] = Some(a);
                self.tags[k] = Some(tag);
            }
        }

        // Screen-pinned CanvasLayer: bottom damage panel. The status chip (formerly top-left) has
        // been moved into the pause menu Network page so the HUD stays clean.
        let mut layer = CanvasLayer::new_alloc();

        // Bottom damage panel: each fighter's name + %, wearing the slot color.
        // Positioned/filled every frame in `update_hud` (handles window resize + active count).
        // The chip doubles as the fighter's edit control: a tap/click opens the pause menu on
        // that slot's CharEdit page (wired to DebugUi below, once its node is resolved -- the
        // raw-event parsing lives there, inside the intent registry's fence).
        for k in 0..sim::MAX_PLAYERS {
            let color = if k == 0 {
                self.identity.get_cloned().color
            } else {
                slot_color(k)
            };
            let mut l = make_hud_label(color);
            l.set_mouse_filter(godot::classes::control::MouseFilter::STOP);
            layer.add_child(&l);
            self.hud[k] = Some(l);
        }

        self.base_mut().add_child(&layer);
        // self.status stays None: status info lives in the pause menu Network page.
        // Sibling Camera2D (authored in game.tscn). We drive it each frame to keep both fighters
        // framed; without this it sits at its static authored transform and fighters leave the view.
        self.cam = self.base().try_get_node_as::<Camera2D>("../Camera2D");
        self.debug_ui = self
            .base()
            .try_get_node_as::<crate::ui::debug::DebugUi>("../DebugUi");
        if let Some(dbg) = self.debug_ui.clone() {
            for (k, l) in self.hud.iter_mut().enumerate() {
                if let Some(l) = l.as_mut() {
                    l.connect(
                        "gui_input",
                        &Callable::from_object_method(&dbg, "on_hud_input")
                            .bind(&[(k as i64).to_variant()]),
                    );
                }
            }
        }

        // Version-check HTTP client: refetches the relay /status on refocus to spot a stale cached
        // web build (web routes this through the browser fetch). Ping once now to catch a stale load.
        let mut http = HttpRequest::new_alloc();
        let vcb = self.to_gd();
        http.connect(
            "request_completed",
            &Callable::from_object_method(&vcb, "on_status_fetched"),
        );
        self.base_mut().add_child(&http);
        self.http = Some(http);
        self.ping_version();

        // Netcode event firehose: its own HttpRequest node (no completion callback -- fire-and-forget
        // POSTs to the relay /ev). On the web export Godot routes this through the browser fetch.
        let mut ev_http = HttpRequest::new_alloc();
        self.base_mut().add_child(&ev_http);
        ev_http.set_name("AnalyticsHttp");
        self.analytics_http = Some(ev_http);
        crate::analytics::log("boot", &format!(r#","build":"{}""#, rtc::BUILD_HASH));

        // TURN prefetch: GET /turn once so ice_config() can add a relay fallback for symmetric-NAT /
        // VPN peer pairs. Its own node + completion callback (unlike the fire-and-forget analytics one)
        // because we need the response body. 404 (TURN unconfigured) leaves us STUN-only, no harm.
        let mut turn_http = HttpRequest::new_alloc();
        let tcb = self.to_gd();
        turn_http.connect(
            "request_completed",
            &Callable::from_object_method(&tcb, "on_turn_fetched"),
        );
        self.base_mut().add_child(&turn_http);
        turn_http.set_name("TurnHttp");
        let _ = turn_http.request(&GString::from(rtc::turn_url().as_str()));
        self.turn_http = Some(turn_http);

        // Skybox: a screen-pinned vertical gradient behind the world (deep space-blue -> horizon
        // glow). CanvasLayer at a negative layer keeps it under the stage + fighters, and a
        // full-rect anchor lets it cover any window without per-frame resizing.
        let mut sky_layer = CanvasLayer::new_alloc();
        sky_layer.set_layer(-10);
        let mut grad = Gradient::new_gd();
        grad.set_offsets(&PackedFloat32Array::from(&[0.0, 0.55, 1.0]));
        grad.set_colors(&PackedColorArray::from(&[
            Color::from_rgb(0.04, 0.05, 0.12), // top of sky
            Color::from_rgb(0.10, 0.13, 0.28), // mid
            Color::from_rgb(0.22, 0.20, 0.34), // horizon haze
        ]));
        let mut tex = GradientTexture2D::new_gd();
        tex.set_gradient(&grad);
        tex.set_fill_from(Vector2::new(0.0, 0.0));
        tex.set_fill_to(Vector2::new(0.0, 1.0)); // vertical
        let mut sky = TextureRect::new_alloc();
        sky.set_texture(&tex);
        sky.set_anchors_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        sky.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
        sky_layer.add_child(&sky);
        self.base_mut().add_child(&sky_layer);

        // Off-stage chips: screen-pinned labels that appear at the screen edge when a fighter is
        // launched out of view, showing name + a pointer arrow + the off-screen distance.
        let mut edge_layer = CanvasLayer::new_alloc();
        edge_layer.set_layer(40);
        for k in 0..sim::MAX_PLAYERS {
            let color = if k == 0 {
                self.identity.get_cloned().color
            } else {
                slot_color(k)
            };
            let chip = make_edge_tag(color);
            edge_layer.add_child(&chip);
            self.edge_tags[k] = Some(chip);
        }
        self.base_mut().add_child(&edge_layer);

        self.build_touch_ui();
        self.update_status();
        self.update_hud();

        // E2E test hook (web builds, gated behind `?autofind=1`/`?script=`; see `crate::webtest`).
        // Always run it (its `?script=`/`?dumpstate=` side effects can matter even when a join link
        // is also present), but a shared `?join=<code>` link takes precedence over its `autofind`:
        // dial straight into that private room (`session::boot_join_code`, `matchmake_room`) instead
        // of firing the same open-matchmaking `find_match` the status chip's tap does. `join` is a
        // PRODUCT feature (shareable lobby link), independent of webtest's test_mode.
        let autofind = crate::webtest::boot();
        if let Some(code) = session::boot_join_code() {
            self.matchmake_room(&code);
        } else if autofind {
            self.on_connect();
        }
    }

    fn physics_process(&mut self, delta: f64) {
        let debug_step = self.debug_tick();
        // MENU pause freezes the LOCAL sim, but a live netplay handshake/session must keep pumping:
        // a lobby opened from the Network page (menu still up) has to finish connecting, and a running
        // match can't stall ggrs. The old early-return here skipped pump_signaling while paused, so a
        // lobby dialed with the menu open parked at the relay forever and neither side ever joined.
        let menu_open = self
            .debug_ui
            .as_ref()
            .map(|d| d.bind().is_menu_open())
            .unwrap_or(false);
        if menu_open {
            self.update_touch(); // still runs while paused so the touch UI hides behind the menu
        }
        match self.phase {
            Phase::Offline => {
                if debug_step || (!menu_open && !self.debugger.paused) {
                    self.step_local();
                }
            }
            // Keep rendering local play while the WebRTC handshake completes, then flip to rollback.
            Phase::Signaling => {
                self.pump_signaling();
                if self.phase != Phase::Running && !menu_open {
                    self.step_local();
                }
            }
            Phase::Running => self.step_net(),
            // Re-pair through the same handshake pump; keep showing local play meanwhile. Give up
            // and free the room once the window elapses with no opponent back.
            Phase::Reconnecting => {
                self.pump_signaling();
                if self.phase == Phase::Reconnecting {
                    let expired = self
                        .room
                        .as_ref()
                        .and_then(|r| r.deadline_ms)
                        .map(|d| now_ms() > d)
                        .unwrap_or(true);
                    if expired {
                        godot_print!("netplay: reconnect window elapsed — turning off");
                        self.reset_offline();
                    } else if !menu_open {
                        self.step_local();
                    }
                }
            }
        }
        self.update_status();
        self.update_hud();
        self.check_pressure();
        self.publish_netdbg();
        self.sync_identity();
        self.sync_charsel();
        self.place_tags();
        self.pump_analytics();
        if !self.debugger.isolated {
            self.persist_ink();
            self.tick_autosave(delta as f32);
        }
        self.debugger.observe(&self.state.get());
        self.publish_webtest();
        if crate::webtest::should_freeze(self.state.get().tick) {
            if let Some(mut tree) = self.base().get_tree() {
                // Pausing broadcasts notifications back into this node. Defer until
                // physics_process releases its mutable GDExtension borrow.
                tree.call_deferred("set_pause", &[true.to_variant()]);
            }
        }
    }

    /// Window/tab regained focus (desktop WM focus or browser tab focus on web). Re-ping the relay
    /// to check whether a deploy left this client running a stale build. The browser tab case is the
    /// one that matters: a tab backgrounded across a redeploy comes back on cached, mismatched wasm.
    ///
    /// BOTH focus edges also release every pressed input (`controls::release_all` -- the stuck-
    /// shield fix; see its doc for the full failure story). Releasing on focus-out covers a focus
    /// steal mid-hold; releasing on focus-in self-heals a keyup eaten while focused.
    fn on_notification(&mut self, what: godot::classes::notify::CanvasItemNotification) {
        use godot::classes::notify::CanvasItemNotification as N;
        if matches!(what, N::WM_WINDOW_FOCUS_IN | N::APPLICATION_FOCUS_IN) {
            self.ping_version();
        }
        if matches!(
            what,
            N::WM_WINDOW_FOCUS_IN
                | N::APPLICATION_FOCUS_IN
                | N::WM_WINDOW_FOCUS_OUT
                | N::APPLICATION_FOCUS_OUT
        ) {
            crate::controls::release_all();
            TOUCH_STICK.set((0.0, 0.0));
            TOUCH_CSTICK.set((0.0, 0.0));
            TOUCH_FINGER.set(-1);
            TOUCH_CSTICK_FINGER.set(-1);
            TOUCH_BTNS.with_borrow_mut(Vec::clear);
        }
    }

    /// Touch gamepad. Screen touches feed the on-screen stick/buttons; the stick writes the
    /// thread-local read by `sample_input`; the buttons press/release the same Input actions the
    /// keyboard binds. (Finding a match is the status chip's `on_connect`, not a key here.)
    fn input(&mut self, event: Gd<InputEvent>) {
        use crate::controls::Touch;
        let Some(touch) = crate::controls::classify_touch(&event) else {
            return;
        };
        match touch {
            // Finger down: claim a face button (right) or the floating stick (left).
            Touch::Down { finger, pos } => {
                // hit-test the shorthop rect, then the diamond wedges, against this frame's layout
                let hit = TOUCH_DIAMOND.get().and_then(|lay| {
                    if lay.shorthop.contains_point(pos) {
                        Some(crate::controls::GameAction::ShortHop.names())
                    } else {
                        quad_at(pos, lay.center, lay.radius).map(|q| QUADS[q].actions)
                    }
                });
                if let Some(actions) = hit {
                    crate::controls::press_actions(actions);
                    TOUCH_BTNS.with_borrow_mut(|v| v.push((finger, actions)));
                } else if TOUCH_FINGER.get() < 0 && pos.x < TOUCH_STICK_ZONE_X.get() {
                    TOUCH_FINGER.set(finger);
                    TOUCH_ORIGIN.set((pos.x, pos.y));
                    TOUCH_STICK.set((0.0, 0.0));
                } else if TOUCH_CSTICK_FINGER.get() < 0 && pos.x >= TOUCH_STICK_ZONE_X.get() {
                    // right-side touch that missed every button: the floating c-stick, wherever
                    // the thumb landed (mirror of the move stick's left-side claim above)
                    TOUCH_CSTICK_FINGER.set(finger);
                    TOUCH_CSTICK_ORIGIN.set((pos.x, pos.y));
                    TOUCH_CSTICK.set((0.0, 0.0));
                }
            }
            // Finger up: drop any wedge this finger held, and free the stick if it owned it.
            Touch::Up { finger } => {
                TOUCH_BTNS.with_borrow_mut(|v| {
                    v.retain(|&(f, actions)| {
                        if f == finger {
                            crate::controls::release_actions(actions);
                            false
                        } else {
                            true
                        }
                    })
                });
                if TOUCH_FINGER.get() == finger {
                    TOUCH_FINGER.set(-1);
                    TOUCH_STICK.set((0.0, 0.0));
                }
                if TOUCH_CSTICK_FINGER.get() == finger {
                    TOUCH_CSTICK_FINGER.set(-1);
                    TOUCH_CSTICK.set((0.0, 0.0)); // snap-back: release IS the smash-flick reset
                }
            }
            // Finger drag: if it owns the stick, update the tilt from its travel off the origin.
            Touch::Drag { finger, pos } => {
                if finger == TOUCH_FINGER.get() {
                    let (ox, oy) = TOUCH_ORIGIN.get();
                    let rad = TOUCH_STICK_RAD.get();
                    let sx = ((pos.x - ox) / rad).clamp(-1.0, 1.0);
                    let sy = ((pos.y - oy) / rad).clamp(-1.0, 1.0);
                    TOUCH_STICK.set((sx, sy));
                } else if finger == TOUCH_CSTICK_FINGER.get() {
                    let (ox, oy) = TOUCH_CSTICK_ORIGIN.get();
                    let rad = TOUCH_CSTICK_RAD.get();
                    let sx = ((pos.x - ox) / rad).clamp(-1.0, 1.0);
                    let sy = ((pos.y - oy) / rad).clamp(-1.0, 1.0);
                    TOUCH_CSTICK.set((sx, sy));
                }
            }
        }
    }

    /// Debug overlay: each fighter's ECB (cyan), hurtbox (yellow), and active hitbox (red).
    /// Drawn for both players so P2's attacks show their boxes too. Coordinates are world,
    /// converted to this node's local space (the node sits at the player position).
    fn draw(&mut self) {
        let s = self.state.get();
        let t = self.tune.get_cloned();
        let active = s.active as usize;
        let origin = self.base().get_position();

        self.detect_blast_bangs(&s, active);
        self.detect_state_fx(&s, active);

        self.draw_blast_walls(origin);
        self.draw_bangs(origin);
        self.draw_puffs(origin);
        self.draw_flashes(origin);
        self.draw_motion_smear(&s, active, origin);
        self.draw_gizmos(&s, &t, active, origin);
        self.draw_telegraph(&s, &t, active, origin);
        self.draw_ac_mechs(&s, active, origin);
        self.draw_items(&s, origin);
        self.draw_fx(&s, origin);
        self.draw_ink_paths(&s, origin);
        self.draw_booster(&s, &t, origin);
        self.draw_gas_meter(&s, active, origin);
        self.draw_aim_arrows(&s, active, origin);
        self.draw_pickup_tooltip(&s, &t, active, origin);
    }
}

impl KneeMan {
    /// Blast bangs: a KO teleports the fighter from a blast edge back to spawn in one frame.
    /// Detect that jump, drop a flash on the boundary they flew through, age the rest out.
    fn detect_blast_bangs(&mut self, s: &SimState, active: usize) {
        for k in 0..active {
            let p = s.fighters[k].pos;
            let prev = self.prev_pos[k];
            if (p - prev).length() > 700.0 {
                let edge = sim::Vector2::new(
                    prev.x.clamp(sim::BLAST_LEFT, sim::BLAST_RIGHT),
                    prev.y.clamp(sim::BLAST_TOP, sim::BLAST_Y),
                );
                self.bangs.push((gv(edge), 0.0));
            }
            self.prev_pos[k] = p;
        }
    }

    /// Dirt-cloud puffs (Task 3) and special-attack flashes (Task 4): detect state transitions.
    /// prev_state/prev_vel trail one draw() call behind (updated at the bottom of this loop).
    fn detect_state_fx(&mut self, s: &SimState, active: usize) {
        for k in 0..active {
            let f = &s.fighters[k];

            // Braking puff: fast ground state (Dash/Run) collapses into a stop (Skid/Stand/Turn).
            let was_fast_ground = matches!(self.prev_state[k], CharState::Dash | CharState::Run);
            let just_braked = matches!(
                f.state,
                CharState::Skid | CharState::Stand | CharState::Turn
            ) && f.state != self.prev_state[k];
            // Landing skid: airborne -> Landing with carry speed above threshold.
            let was_air = matches!(
                self.prev_state[k],
                CharState::Air
                    | CharState::Nair
                    | CharState::Dair
                    | CharState::AirDodge
                    | CharState::Helpless
            );
            let landed_fast = f.state == CharState::Landing
                && self.prev_state[k] != CharState::Landing
                && self.prev_vel[k].x.abs() > 4.0;
            if (was_fast_ground && just_braked) || (was_air && landed_fast) {
                self.puffs.push((gv(f.pos), 0.0));
            }

            // Special flash: entering any B-move from a non-special state.
            let now_special = matches!(
                f.state,
                CharState::SpecialN
                    | CharState::SpecialS
                    | CharState::SpecialU
                    | CharState::SpecialD
            );
            let was_special = matches!(
                self.prev_state[k],
                CharState::SpecialN
                    | CharState::SpecialS
                    | CharState::SpecialU
                    | CharState::SpecialD
            );
            if now_special && !was_special {
                // Body center: feet pos lifted by the body half-height (46 px up = -y in Godot).
                let body_center = gv(f.pos) - Vector2::new(0.0, 46.0);
                self.flashes.push((body_center, 0.0));
            }

            self.prev_state[k] = f.state;
            self.prev_vel[k] = f.vel;
        }
    }

    /// Blast bangs: draw + age each bang, an expanding ring plus radiating spokes, hot orange
    /// fading out.
    fn draw_bangs(&mut self, origin: Vector2) {
        self.bangs.retain(|(_, age)| *age < 1.0);
        let bangs: Vec<(Vector2, f32)> = self.bangs.clone();
        for (i, (wp, age)) in bangs.iter().enumerate() {
            let c = *wp - origin;
            let a = *age;
            let r = 30.0 + a * 230.0;
            let alpha = (1.0 - a).powf(1.4);
            let col = Color::from_rgba(1.0, 0.55 + 0.35 * (1.0 - a), 0.12, alpha);
            self.base_mut()
                .draw_arc_ex(c, r, 0.0, std::f32::consts::TAU, 28, col)
                .width(6.0 * (1.0 - a) + 1.0)
                .done();
            for spoke in 0..8 {
                let ang = spoke as f32 / 8.0 * std::f32::consts::TAU + a * 0.6;
                let dir = Vector2::new(ang.cos(), ang.sin());
                self.base_mut()
                    .draw_line_ex(c + dir * (r * 0.5), c + dir * (r + 40.0 * (1.0 - a)), col)
                    .width(5.0 * (1.0 - a) + 1.0)
                    .done();
            }
            self.bangs[i].1 = a + 0.045;
        }
    }

    /// Dirt-cloud puffs (Task 3): 3 expanding tan circles that drift upward, fade at the feet.
    fn draw_puffs(&mut self, origin: Vector2) {
        self.puffs.retain(|(_, age)| *age < 1.0);
        let puffs: Vec<(Vector2, f32)> = self.puffs.clone();
        for (i, (wp, age)) in puffs.iter().enumerate() {
            let c = *wp - origin;
            let a = *age;
            let alpha = (1.0 - a).powf(1.8);
            // Three circles with slight lateral offsets; all drift upward as age advances.
            for (dx, scale) in [(-9.0_f32, 0.80_f32), (0.0, 1.0), (10.0, 0.72)] {
                let r = (7.0 + a * 24.0) * scale;
                let rise = a * 14.0; // puff drifts up over its lifetime
                let col = Color::from_rgba(0.75, 0.63, 0.38, alpha * 0.62);
                self.base_mut()
                    .draw_circle(c + Vector2::new(dx * (1.0 + a * 0.4), -rise), r, col);
            }
            self.puffs[i].1 = a + 0.07;
        }
    }

    /// Special-attack flashes (Task 4): bright expanding ring + inner glow at body center.
    fn draw_flashes(&mut self, origin: Vector2) {
        self.flashes.retain(|(_, age)| *age < 1.0);
        let flashes: Vec<(Vector2, f32)> = self.flashes.clone();
        for (i, (wp, age)) in flashes.iter().enumerate() {
            let c = *wp - origin;
            let a = *age;
            let alpha = (1.0 - a).powf(1.2);
            let r = 18.0 + a * 52.0;
            let ring_col = Color::from_rgba(0.95, 0.88, 0.42, alpha * 0.82);
            self.base_mut()
                .draw_arc_ex(c, r, 0.0, std::f32::consts::TAU, 24, ring_col)
                .width(5.0 * (1.0 - a) + 1.5)
                .done();
            // Inner glow disc fades faster than the ring.
            let glow_col = Color::from_rgba(1.0, 0.95, 0.60, alpha * alpha * 0.45);
            self.base_mut().draw_circle(c, r * 0.55, glow_col);
            self.flashes[i].1 = a + 0.06;
        }
    }

    /// Motion smear: fast bursts (up-B / side-B / a hard launch) move a frozen single-frame
    /// sprite far enough per frame that the eye reads it as a teleport. Trail a few fading
    /// ghost discs along the recent path so the movement reads as motion instead of a pop.
    /// Purely cosmetic (shell-side), never touches the sim or the netplay checksum.
    fn draw_motion_smear(&mut self, s: &SimState, active: usize, origin: Vector2) {
        for k in 0..active {
            let p = s.fighters[k].pos;
            let trail = &mut self.trails[k];
            trail.push(p);
            if trail.len() > 6 {
                trail.remove(0);
            }
            // speed = last per-frame step. Below ~9px/frame (a normal run) draw nothing.
            let speed = trail
                .last()
                .zip(trail.get(trail.len().wrapping_sub(2)))
                .map(|(a, b)| (*a - *b).length())
                .unwrap_or(0.0);
            if speed < 9.0 {
                continue;
            }
            let body = 46.0_f32; // body half-height, in world px (lift the disc to torso level)
            let intensity = ((speed - 9.0) / 26.0).clamp(0.0, 1.0); // 9..35 px/frame -> 0..1
            let pts: Vec<sim::Vector2> = trail.clone(); // drop the &mut self.trails borrow before drawing
            let n = pts.len();
            for (j, gp) in pts.iter().enumerate() {
                let f = j as f32 / (n.max(2) - 1) as f32; // 0 oldest .. 1 newest
                let c = gv(*gp) - origin - Vector2::new(0.0, body); // lift to body center
                let alpha = (0.30 * intensity) * f * f; // fade hard toward the tail
                let col = Color::from_rgba(0.75, 0.88, 1.0, alpha);
                self.base_mut()
                    .draw_circle(c, body * (0.55 + 0.35 * f), col);
            }
        }
    }

    /// ECB / hurtbox / hitbox boxes: opt-in debug overlay, gated by the panel's "show hitboxes"
    /// checkbox (the `gizmos` cell). Off by default; the items/ink/juice below always draw.
    fn draw_gizmos(&mut self, s: &SimState, t: &Tune, active: usize, origin: Vector2) {
        if self.gizmos.get() {
            let ecb = Color::from_rgba(0.20, 0.85, 0.95, 0.85);
            let hurt_col = Color::from_rgba(0.95, 0.85, 0.20, 0.30);
            let hit_col = Color::from_rgba(0.95, 0.25, 0.25, 0.45);
            for f in &s.fighters[..active] {
                // ECB diamond: the actual collision shape — bottom vert = feet, side verts = walls.
                // On a ledge hang the sim pins pos.x to the wall line itself, so a symmetric diamond
                // straddles the wall (half buried in the stage). Shift it outward (away from facing,
                // i.e. the open side the body hangs on) by ECB_HALF_W so the inner vert meets the lip.
                let draw_pos = if matches!(f.state, CharState::LedgeHold | CharState::LedgeClimb) {
                    f.pos - sim::Vector2::new(f.facing * sim::ECB_HALF_W, 0.0)
                } else {
                    f.pos
                };
                let v = sim::ecb_verts(draw_pos);
                for k in 0..4 {
                    let a = gv(v[k]) - origin;
                    let b = gv(v[(k + 1) % 4]) - origin;
                    self.base_mut().draw_line_ex(a, b, ecb).width(2.0).done();
                }
                // hurtbox: the circle an attack lands on.
                let (bc, br) = sim::hurtbox(f);
                let hurt = gv(bc) - origin;
                self.base_mut().draw_circle(hurt, br, hurt_col);
                // active hitboxes: every box live this frame (a multi-box move shows all its windows).
                for hb in sim::live_hitboxes(f, t).into_iter().flatten() {
                    let (hc, hr) = hb;
                    let c = gv(hc) - origin;
                    self.base_mut().draw_circle(c, hr, hit_col);
                }
            }
        } // end gizmos overlay
    }

    /// DAIR + B-move telegraph: always draw the live hitboxes for Dair and the four specials
    /// (SpecialN/S/U/D) so those moves read even with gizmos off -- the specials have no bespoke
    /// anim frames, so the hitbox IS the read. Skip when gizmos is on: already drawn above.
    fn draw_telegraph(&mut self, s: &SimState, t: &Tune, active: usize, origin: Vector2) {
        if !self.gizmos.get() {
            let tele_col = Color::from_rgba(1.0, 0.45, 0.05, 0.55);
            for f in &s.fighters[..active] {
                let telegraphed = matches!(
                    f.state,
                    CharState::Dair
                        | CharState::SpecialN
                        | CharState::SpecialS
                        | CharState::SpecialU
                        | CharState::SpecialD
                );
                if telegraphed {
                    for hb in sim::live_hitboxes(f, t).into_iter().flatten() {
                        let (hc, hr) = hb;
                        let c = gv(hc) - origin;
                        self.base_mut().draw_circle(c, hr, tele_col);
                    }
                }
            }
        }
    }

    /// Armored Core bodies: `render_fighter` hid the AnimatedSprite2D for any fighter
    /// wearing the AcCore badge, so paint the boxy mech here instead — one state read,
    /// one shell draw, the sim never hears about it.
    fn draw_ac_mechs(&mut self, s: &SimState, active: usize, origin: Vector2) {
        for k in 0..active {
            let f = s.fighters[k];
            if f.has_badge(sim::Badge::AcCore) {
                let tint = self.slot_tint(k);
                self.draw_mech(&f, tint, origin);
            }
        }
    }

    /// items + projectiles (debug shapes for now; model_id -> sprite is the later polish)
    fn draw_items(&mut self, s: &SimState, origin: Vector2) {
        for it in &s.items {
            if !it.active() {
                continue;
            }
            let c = gv(it.pos) - origin;
            match it.kind {
                sim::ItemKind::TerrainCell => {
                    let size = Vector2::new(30.0, 30.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.95, 0.65, 0.22),
                    );
                }
                sim::ItemKind::LaserGun => {
                    let size = Vector2::new(38.0, 16.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.25, 0.95, 0.45),
                    );
                }
                sim::ItemKind::LaserBolt => {
                    let half = Vector2::new(20.0 * it.facing, 0.0);
                    self.base_mut()
                        .draw_line_ex(c - half, c + half, Color::from_rgb(1.0, 0.25, 0.20))
                        .width(6.0)
                        .done();
                }
                sim::ItemKind::BobGun => {
                    let size = Vector2::new(40.0, 18.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.92, 0.16, 0.16), // red gun
                    );
                }
                sim::ItemKind::Bomb => {
                    // dark body with a red fuse-glow ring, so the lobbed Bob-omb reads in the air.
                    self.base_mut()
                        .draw_circle(c, 14.0, Color::from_rgb(0.08, 0.08, 0.10));
                    self.base_mut()
                        .draw_arc_ex(
                            c,
                            18.0,
                            0.0,
                            std::f32::consts::TAU,
                            20,
                            Color::from_rgb(1.0, 0.4, 0.15),
                        )
                        .width(3.0)
                        .done();
                }
                sim::ItemKind::Pen => {
                    // drawing tool pickup: a bright nib so it reads as ink.
                    let size = Vector2::new(30.0, 30.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.20, 0.55, 1.0),
                    );
                }
                sim::ItemKind::TetrisGun => {
                    // tetromino lobber: a 2x2 mini-O in the piece hue so it reads as "block gun".
                    let cell = 11.0;
                    for (dx, dy) in [(0.0, 0.0), (cell, 0.0), (0.0, cell), (cell, cell)] {
                        self.base_mut().draw_rect(
                            Rect2::new(
                                c + Vector2::new(dx - cell, dy - cell),
                                Vector2::new(cell - 1.0, cell - 1.0),
                            ),
                            Color::from_rgb(0.95, 0.45, 0.95),
                        );
                    }
                }
                sim::ItemKind::InkGun => {
                    // drawn-shot gun: a gun-shaped body in the ink hue so it reads as both.
                    let size = Vector2::new(38.0, 16.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.20, 0.55, 1.0),
                    );
                    self.base_mut().draw_circle(
                        c + Vector2::new(12.0, 0.0),
                        5.0,
                        Color::from_rgb(0.9, 0.97, 1.0),
                    );
                }
                sim::ItemKind::WingsBadge => {
                    // badge pickup: a gold coin with a white wing-flick either side.
                    self.base_mut()
                        .draw_circle(c, 15.0, Color::from_rgb(0.98, 0.82, 0.25));
                    for side in [-1.0f32, 1.0] {
                        let root = c + Vector2::new(13.0 * side, -2.0);
                        let tip = c + Vector2::new(26.0 * side, -14.0);
                        self.base_mut()
                            .draw_line_ex(root, tip, Color::from_rgb(0.97, 0.97, 1.0))
                            .width(5.0)
                            .done();
                    }
                }
                sim::ItemKind::AcCore => {
                    // the mech core sitting on the ground: dark slate body with a cyan visor slit.
                    let size = Vector2::new(40.0, 34.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.16, 0.19, 0.24),
                    );
                    self.base_mut()
                        .draw_line_ex(
                            c + Vector2::new(-12.0, -6.0),
                            c + Vector2::new(12.0, -6.0),
                            Color::from_rgb(0.30, 0.90, 1.0),
                        )
                        .width(4.0)
                        .done();
                }
                sim::ItemKind::Rocket => {
                    // bazooka round: a small grey capsule with an orange tail streaking behind it.
                    let tail = it.pos - it.vel.normalize_or_zero() * 26.0;
                    self.base_mut()
                        .draw_line_ex(gv(tail) - origin, c, Color::from_rgb(1.0, 0.5, 0.12))
                        .width(4.0)
                        .done();
                    self.base_mut()
                        .draw_circle(c, 9.0, Color::from_rgb(0.6, 0.62, 0.66));
                }
                sim::ItemKind::PlasmaBall => {
                    // energy cannon round: a fat cyan orb with a brighter core.
                    self.base_mut()
                        .draw_circle(c, 14.0, Color::from_rgb(0.20, 0.75, 0.95));
                    self.base_mut()
                        .draw_circle(c, 6.0, Color::from_rgb(0.80, 0.98, 1.0));
                }
                sim::ItemKind::TetrisDropper => {
                    // TetrisGun's sibling: the same mini-O in a cooler hue, plus a small down
                    // arrow so the two read as "lob" vs "drop" at a glance.
                    let cell = 11.0;
                    for (dx, dy) in [(0.0, 0.0), (cell, 0.0), (0.0, cell), (cell, cell)] {
                        self.base_mut().draw_rect(
                            Rect2::new(
                                c + Vector2::new(dx - cell, dy - cell),
                                Vector2::new(cell - 1.0, cell - 1.0),
                            ),
                            Color::from_rgb(0.40, 0.65, 0.95),
                        );
                    }
                    self.base_mut()
                        .draw_line_ex(
                            c + Vector2::new(0.0, 8.0),
                            c + Vector2::new(0.0, 20.0),
                            Color::from_rgb(0.40, 0.65, 0.95),
                        )
                        .width(3.0)
                        .done();
                }
                sim::ItemKind::Station => {
                    // mounted ship station (plans/lovers-ship.md): a console pad on the hull. A
                    // slate box with a warm indicator so a rider reads it as "interact to crew".
                    let size = Vector2::new(36.0, 26.0);
                    self.base_mut().draw_rect(
                        Rect2::new(c - size * 0.5, size),
                        Color::from_rgb(0.22, 0.24, 0.30),
                    );
                    self.base_mut().draw_circle(
                        c + Vector2::new(0.0, -2.0),
                        6.0,
                        Color::from_rgb(1.0, 0.78, 0.30),
                    );
                }
                sim::ItemKind::None => {}
            }
        }
    }

    /// Fx ring: sim-emitted cosmetic events (explosions, muzzle flashes, transform poofs).
    /// A fresh match's untouched slots are `FxKind::None` (tick 0) and skip below, so an
    /// empty slot never misreads as an ancient event. Shitty and funny on purpose.
    fn draw_fx(&mut self, s: &SimState, origin: Vector2) {
        for fx in &s.fx {
            if fx.kind == sim::FxKind::None {
                continue;
            }
            let age = s.tick.saturating_sub(fx.tick) as f32;
            let c = gv(fx.pos) - origin;
            match fx.kind {
                sim::FxKind::Explosion => {
                    let life = 24.0_f32;
                    if age >= life {
                        continue;
                    }
                    let a = age / life;
                    let alpha = (1.0 - a).powf(1.3);
                    let r = 12.0 + a * 58.0;
                    self.base_mut().draw_circle(
                        c,
                        r,
                        Color::from_rgba(1.0, 0.42, 0.08, alpha * 0.55),
                    );
                    self.base_mut().draw_circle(
                        c,
                        r * 0.42,
                        Color::from_rgba(1.0, 0.92, 0.35, alpha * 0.85),
                    );
                    for spike in 0..6u64 {
                        let jit = fx_jitter(fx.tick, spike);
                        let ang = spike as f32 / 6.0 * std::f32::consts::TAU + jit * 0.8;
                        let dir = Vector2::new(ang.cos(), ang.sin());
                        let len = r * (0.7 + jit * 0.9);
                        self.base_mut()
                            .draw_line_ex(
                                c + dir * r * 0.5,
                                c + dir * len,
                                Color::from_rgba(1.0, 0.6, 0.15, alpha),
                            )
                            .width(3.0)
                            .done();
                    }
                }
                sim::FxKind::Muzzle => {
                    let life = 5.0_f32;
                    if age >= life {
                        continue;
                    }
                    let a = age / life;
                    let alpha = 1.0 - a;
                    let r = 5.0 + a * 7.0;
                    for ang in [0.0_f32, std::f32::consts::FRAC_PI_2] {
                        let dir = Vector2::new(ang.cos(), ang.sin());
                        self.base_mut()
                            .draw_line_ex(
                                c - dir * r,
                                c + dir * r,
                                Color::from_rgba(1.0, 0.95, 0.55, alpha),
                            )
                            .width(3.0)
                            .done();
                    }
                    self.base_mut().draw_circle(
                        c,
                        r * 0.4,
                        Color::from_rgba(1.0, 1.0, 0.85, alpha),
                    );
                }
                sim::FxKind::Transform => {
                    let life = 30.0_f32;
                    if age >= life {
                        continue;
                    }
                    let a = age / life;
                    let alpha = (1.0 - a).powf(1.2);
                    for ring in 0..2u32 {
                        let delay = ring as f32 * 0.35;
                        let ra = a - delay;
                        if ra <= 0.0 {
                            continue;
                        }
                        let r = 18.0 + ra * 120.0;
                        self.base_mut()
                            .draw_arc_ex(
                                c,
                                r,
                                0.0,
                                std::f32::consts::TAU,
                                26,
                                Color::from_rgba(1.0, 1.0, 1.0, alpha * (1.0 - delay * 0.5)),
                            )
                            .width((4.0 * (1.0 - a) + 1.0).max(1.0))
                            .done();
                    }
                }
                sim::FxKind::Fire => self.draw_fire(c, age, fx.tick),
                sim::FxKind::None => {}
            }
        }
    }

    /// drawn ink paths: stroke each live polyline. Cosmetic read of the sim's cached classes —
    /// grabbable lips get a hotter tint so the playable surface is legible.
    // parity(v1-ink-path-presentation): every live path renders its current world polyline, class color and thickness, strike shake, anchored close target, and finite-life countdown from simulation state
    fn draw_ink_paths(&mut self, s: &SimState, origin: Vector2) {
        for (pi, p) in s.paths.iter().enumerate() {
            if !p.active() {
                continue;
            }
            // struck-but-not-launched ink jiggles for `shake` frames (the ink "hitstun"): a small
            // per-path phase-offset wobble, cosmetic only — the sim geometry never moves.
            let jit = if p.shake > 0 {
                let ph = s.tick as f32 * 1.9 + pi as f32 * 2.1;
                Vector2::new(ph.sin() * 2.5, (ph * 1.7).cos() * 2.5)
            } else {
                Vector2::ZERO
            };
            let n = p.len as usize;
            for seg in 0..n.saturating_sub(1) {
                let a = gv(p.world_pt(seg, &s.nodes)) - origin + jit;
                let b = gv(p.world_pt(seg + 1, &s.nodes)) - origin + jit;
                // While the owner is still laying the stroke, every segment is class None (classify
                // runs at finalize), so a live draw would read as a weak faded line. Paint it solid
                // ink instead so the drawing shows up live under the pen, then it recolors to its
                // Floor/Ledge/Wall class the moment the stroke is released.
                // (col, width). `None` segments are laid ink that classified as no usable surface
                // (too short, or a ramp too steep to stand / too shallow to be a wall). Draw them as
                // a thin faint sketch so they read as "ineffective ink", not a broken platform.
                let (col, width) = if p.drawing {
                    (Color::from_rgb(0.3, 0.7, 1.0), 7.0) // live ink: confident, same hue as a Floor
                } else {
                    match p.seg_class(seg, &s.nodes) {
                        sim::SegClass::Ledge => (Color::from_rgb(1.0, 0.85, 0.2), 7.0), // grabbable lip
                        sim::SegClass::Floor => (Color::from_rgb(0.3, 0.7, 1.0), 7.0),
                        sim::SegClass::Wall => (Color::from_rgb(0.6, 0.4, 1.0), 7.0),
                        sim::SegClass::None => (Color::from_rgba(0.5, 0.8, 1.0, 0.22), 3.0),
                    }
                };
                self.draw_ink_segment(p, seg, a, b, col, width, &s.nodes); // solid = class outside / purple inside
                self.draw_gate_ticks(p, seg, origin, jit, &s.nodes); // one-way pass-side markers
            }

            // Start-dot: an anchored (ink gun) draw highlights its first node — the close-loop
            // target. Drawing back inside the ring snaps the shape shut (INK_CLOSE_R in the sim).
            if p.drawing && p.anchor >= 0 && n > 0 {
                let dot = gv(p.world_pt(0, &s.nodes)) - origin + jit;
                self.base_mut()
                    .draw_circle(dot, 5.0, Color::from_rgb(1.0, 0.85, 0.2));
                self.base_mut()
                    .draw_arc_ex(
                        dot,
                        sim::INK_CLOSE_R,
                        0.0,
                        std::f32::consts::TAU,
                        16,
                        Color::from_rgba(1.0, 0.85, 0.2, 0.6),
                    )
                    .width(2.0)
                    .done();
            }

            // Countdown tag: a FINISHED drawn stroke (owner >= 0, not still being laid) shows how many
            // seconds until it exits, as a small superscript box above its highest node. Baked stage
            // strokes (owner < 0) are permanent and get no tag. Drawn in this ink pass = background z.
            if !p.drawing && p.owner >= 0 {
                let remaining = p.props.stroke_life
                    - s.tick.saturating_sub(p.node_born(n - 1, &s.nodes)) as i64;
                if remaining > 0 {
                    let top = (0..n)
                        .map(|k| p.world_pt(k, &s.nodes))
                        .reduce(|a, b| if a.y <= b.y { a } else { b })
                        .unwrap_or(p.world_pt(0, &s.nodes));
                    let secs = remaining as f32 / 60.0;
                    let text = format!("{secs:.1}s");
                    let fs = 14;
                    let pos = gv(top)
                        - origin
                        - Vector2::new((text.len() as f32) * fs as f32 * 0.3, 14.0);
                    let box_w = text.len() as f32 * fs as f32 * 0.62 + 8.0;
                    let box_tl = pos + Vector2::new(-4.0, -(fs as f32));
                    self.base_mut().draw_rect(
                        Rect2::new(box_tl, Vector2::new(box_w, fs as f32 + 5.0)),
                        Color::from_rgba(0.05, 0.08, 0.12, 0.6),
                    );
                    if let Some(font) = ThemeDb::singleton().get_fallback_font() {
                        self.base_mut()
                            .draw_string_ex(&font, pos, &text)
                            .font_size(fs)
                            .modulate(Color::from_rgba(0.7, 0.9, 1.0, 0.85))
                            .done();
                    }
                }
            }
        }
    }

    /// Ship booster: the engine marker sits on the rim OPPOSITE the helm's aim (flame out the
    /// back). Idle it's a dim nub so the station reads even untouched; under throttle it grows
    /// an exhaust plume matching the sim's blast sector (booster_len reach, ±30°). Cosmetic
    /// read of `s.helm` + the hull stroke's pos — geometry mirrors `booster_blast` in core.
    fn draw_booster(&mut self, s: &SimState, t: &Tune, origin: Vector2) {
        let hull = &s.paths[sim::SHIP_SLOT];
        if hull.len > 0 {
            let center = gv(hull.pos) - origin;
            let ex = gv(-s.helm.aim); // exhaust axis, unit (sim y-down == draw y-down)
            let side = Vector2::new(-ex.y, ex.x);
            let rim = center + ex * sim::SHIP_R;
            let th = s.helm.thrust;
            // the nozzle: two short rails bracketing the exhaust sector's mouth
            let nozzle = Color::from_rgb(0.95, 0.62, 0.2);
            for sgn in [-1.0_f32, 1.0] {
                let m = center + (ex + side * (sgn * 0.5)).normalized() * sim::SHIP_R;
                self.base_mut()
                    .draw_line_ex(m, m + (m - center).normalized() * 16.0, nozzle)
                    .width(5.0)
                    .done();
            }
            if th > 0.0 {
                // plume: a fan of rays across the sector, longest on-axis
                let reach = t.booster_len * th;
                for (k, spread) in [-0.45_f32, -0.22, 0.0, 0.22, 0.45].iter().enumerate() {
                    let dir = (ex + side * *spread).normalized();
                    let len = reach * (1.0 - spread.abs());
                    let hot = k == 2;
                    let col = if hot {
                        Color::from_rgba(1.0, 0.85, 0.3, 0.95)
                    } else {
                        Color::from_rgba(1.0, 0.55, 0.15, 0.7)
                    };
                    self.base_mut()
                        .draw_line_ex(rim, rim + dir * len, col)
                        .width(if hot { 7.0 } else { 4.0 })
                        .done();
                }
            }
        }
    }

    /// Overhead gas meter: while a fighter holds an item, a tiny bar over the head shows its
    /// remaining use (`gas / gas_max`) -- gun shots, or a pen's ink. Cosmetic read of the sim;
    /// `gas` is the item's general first-dimension use measure (see core `Item`).
    fn draw_gas_meter(&mut self, s: &SimState, active: usize, origin: Vector2) {
        for k in 0..active {
            let f = s.fighters[k];
            if f.holding < 0 {
                continue;
            }
            let it = s.items[f.holding as usize];
            if !it.active() || it.gas_max <= 0.0 {
                continue;
            }
            let frac = (it.gas / it.gas_max).clamp(0.0, 1.0);
            let w = 54.0_f32;
            let h = 7.0_f32;
            let head = gv(f.pos) - origin - Vector2::new(w * 0.5, 118.0);
            self.base_mut().draw_rect(
                Rect2::new(
                    head - Vector2::new(1.0, 1.0),
                    Vector2::new(w + 2.0, h + 2.0),
                ),
                Color::from_rgba(0.0, 0.0, 0.0, 0.55), // dark backing so it reads on any stage
            );
            // fill tints green (full) -> amber -> red (nearly spent) as it drains.
            let fill = Color::from_rgb(1.0 - frac * 0.45, 0.35 + frac * 0.55, 0.22);
            self.base_mut()
                .draw_rect(Rect2::new(head, Vector2::new(w * frac, h)), fill);
        }
    }

    /// Aim-arrow tell: a held item whose row aims shows where the shot would go. The
    /// local c-stick drives player 0's arrow live; everyone else's reads movement,
    /// falling back to facing (the neutral fire direction). Cosmetic sim read only.
    /// TODO(body-bus): use the net-local handle so the guest's own arrow is stick-live.
    fn draw_aim_arrows(&mut self, s: &SimState, active: usize, origin: Vector2) {
        for k in 0..active {
            let f = s.fighters[k];
            if f.holding < 0 {
                continue;
            }
            let it = s.items[f.holding as usize];
            if !it.active() || !sim::item_logic(it.kind).aims {
                continue;
            }
            let stick = if k == 0 {
                self.last_aim
            } else {
                sim::Vector2::new(0.0, 0.0)
            };
            let dir = if stick.length() >= 0.4 {
                stick.normalize_or_zero()
            } else if f.vel.length() > 40.0 {
                f.vel.normalize_or_zero()
            } else {
                sim::Vector2::new(f.facing, 0.0)
            };
            let mid = f.pos + sim::Vector2::new(0.0, -70.0); // body center, feet-anchored pos
            let from = gv(mid + dir * 46.0) - origin;
            let tip = gv(mid + dir * 96.0) - origin;
            let col = Color::from_rgba(1.0, 1.0, 1.0, 0.85);
            self.base_mut()
                .draw_line_ex(from, tip, col)
                .width(3.0)
                .done();
            // two head strokes, swept back from the tip
            let side = sim::Vector2::new(-dir.y, dir.x);
            for s in [1.0f32, -1.0] {
                let wing = gv(mid + dir * 78.0 + side * (10.0 * s)) - origin;
                self.base_mut()
                    .draw_line_ex(tip, wing, col)
                    .width(3.0)
                    .done();
            }
        }
    }
}

#[godot_api]
impl KneeMan {
    /// WebRTC fired a local description (offer/answer) for the connection bound to `peer` (host's
    /// own handle for the k<=2 path; see `mesh::on_sdp_created` for the full k>2 story). Set it
    /// locally and relay it to the peer through the signaling socket.
    #[func]
    fn on_sdp_created(&mut self, sdp_type: GString, sdp: GString, peer: i64) {
        mesh::on_sdp_created(self, sdp_type, sdp, peer);
    }

    /// Status-chip tap handler: find a match. Guarded to Offline (a no-op once connecting/connected).
    #[func]
    fn on_connect(&mut self) {
        if self.phase == Phase::Offline {
            crate::analytics::log("find_match", &format!(r#","build":"{}""#, rtc::BUILD_HASH));
            self.start_matchmaking();
        }
    }

    /// MENU tab handler: toggle the XP pause menu (same effect as pressing Escape).
    #[func]
    fn on_menu(&mut self) {
        if let Some(dbg) = self.debug_ui.as_mut() {
            dbg.bind_mut().open_pause_menu();
        }
    }

    /// WebRTC found a local ICE candidate for the connection bound to `peer`. Relay it to the peer.
    #[func]
    fn on_ice_created(&mut self, media: GString, index: i32, name: GString, peer: i64) {
        mesh::on_ice_created(self, media, index, name, peer);
    }

    /// Relay /status fetched (on focus-in / startup): compare the live server's `build_hash` to ours.
    /// A mismatch means our wasm is stale (a deploy happened) -- flag it so the status line says reload.
    #[func]
    fn on_status_fetched(
        &mut self,
        _result: i64,
        code: i64,
        _headers: PackedStringArray,
        body: PackedByteArray,
    ) {
        if code != 200 {
            return; // server unreachable; leave the prior verdict untouched
        }
        let text = body.get_string_from_utf8();
        let server = rtc::dget_str(&rtc::parse_json(&text), "build_hash");
        // Only call it stale when both hashes are real and differ; "unknown" (dev build) never warns.
        self.stale_build = server.starts_with("src-") && server != rtc::BUILD_HASH;
    }

    /// `/turn` fetched at boot: cache the coturn REST credential so `ice_config()` adds a relay
    /// fallback. 404 = TURN unconfigured on the relay -> stay STUN-only. Logged to the firehose so the
    /// tail confirms whether each device armed TURN before a match.
    #[func]
    fn on_turn_fetched(
        &mut self,
        _result: i64,
        code: i64,
        _headers: PackedStringArray,
        body: PackedByteArray,
    ) {
        if code != 200 {
            crate::analytics::log("turn", &format!(r#","ok":false,"code":{code}"#));
            return;
        }
        let text = body.get_string_from_utf8();
        rtc::store_turn_creds(&text);
        let n = rtc::turn_url_count();
        crate::analytics::log("turn", &format!(r#","ok":true,"urls":{n}"#));
    }
}

impl KneeMan {
    /// Hand out the shared cells (clones point at the same BehaviorSubject).
    pub fn state_cell(&self) -> Mutable<SimState> {
        self.state.clone()
    }

    pub fn tune_cell(&self) -> Mutable<Tune> {
        self.tune.clone()
    }

    pub fn gizmos_cell(&self) -> Mutable<bool> {
        self.gizmos.clone()
    }

    pub fn net_cell(&self) -> Mutable<NetDebug> {
        self.netdbg.clone()
    }

    /// Network page (pause menu) actions, mirroring the on-screen status chip: start matchmaking
    /// (no-op unless Offline, like the chip's tap) and drop back to local play from any net phase.
    pub fn find_match(&mut self) {
        self.on_connect();
    }

    pub fn leave_match(&mut self) {
        self.reset_offline();
    }

    /// Shareable quick-match invite (Network page's "Copy invite link"): mint a readable room code,
    /// host it via `matchmake_room` (this player becomes the room's host, same as `OpenLobby`), build
    /// the `?join=` link off the serving origin, and copy it to the clipboard. Guarded to Offline, no-
    /// op mid-match -- like `on_connect`, minting/copying a link for a room we're not hosting would
    /// just hand out a dead invite.
    pub fn invite(&mut self) {
        if self.phase != Phase::Offline {
            return;
        }
        let code = crate::net::mint_invite_code();
        self.matchmake_room(&code);
        let base =
            crate::net::page_directory().unwrap_or_else(|| format!("{}/game3/", rtc::relay_base()));
        let link = rtc::invite_link(&base, &code);
        crate::net::copy_to_clipboard(&link);
        crate::toast::push(
            &self.toasts,
            crate::toast::ToastKind::Info,
            "Invite link copied",
        );
    }

    pub fn identity_cell(&self) -> Mutable<Identity> {
        self.identity.clone()
    }

    pub fn charsel_cell(&self) -> Mutable<[i64; 2]> {
        self.charsel.clone()
    }

    pub fn toasts_cell(&self) -> crate::toast::Toasts {
        self.toasts.clone()
    }

    /// Which fighter slot is the local player (0 offline/host, 1 as guest). For the menu's
    /// CharEdit page: name edits apply to this slot only.
    pub fn local_slot(&self) -> usize {
        self.local_handle
    }

    /// Armored Core mech body: `mech_tex` (the kit-model image at `MECH_TEX`) if one loaded, else the
    /// legacy painted boxes. Feet-anchored at `f.pos`, ~140px tall to fill the same silhouette the
    /// hidden AnimatedSprite2D would have. `tint` is the player's slot color (modulated onto the
    /// body only, so everyone keeps their color even body-swapped); `origin` is the node position
    /// already subtracted everywhere else in `draw()`. The shoulder pod / weapon label / thruster
    /// fx below are separate readable layers (weapon roll, altitude) and stay painted either way.
    fn draw_mech(&mut self, f: &sim::Fighter, tint: Color, origin: Vector2) {
        let c = gv(f.pos) - origin;
        let face = if f.facing < 0.0 { -1.0_f32 } else { 1.0 };

        // Body sizing, shared by both the PNG and the painted fallback so the shoulder pod below
        // (which anchors off `torso_w`/`torso_top`) lines up identically either way.
        let leg_w = 15.0_f32;
        let leg_h = 46.0_f32;
        let torso_w = 60.0_f32;
        let torso_h = 58.0_f32;
        let torso_top = -leg_h - torso_h;
        let head_w = 26.0_f32;
        let head_h = 32.0_f32;
        let head_top = torso_top - head_h;

        if let Some(tex) = self.mech_tex.clone() {
            // Fit the sprite to its own aspect ratio instead of stretching it into the torso box:
            // height tracks the same head-to-feet span the painted fallback fills (times
            // MECH_SCALE -- hand-tune that constant, not this math), width follows tex_w/tex_h so
            // nothing squashes. Feet-anchored: the box's bottom edge stays pinned at `c.y == 0`
            // (same line the painted legs bottom out on) regardless of how tall MECH_SCALE makes it.
            let tex_w = tex.get_width() as f32;
            let tex_h = tex.get_height() as f32;
            let unscaled_h = -head_top; // legs+torso+head span, same box the painted fallback fills
            let body_h = unscaled_h * MECH_SCALE;
            let body_w = if tex_h > 0.0 {
                body_h * (tex_w / tex_h)
            } else {
                torso_w // degenerate (0-height) texture: fall back to the torso width
            };

            // Flip via a mirror transform, not a negated rect width: Godot's draw_texture_rect
            // takes the source art's negative-size axis as a "flip this way" flag but keeps
            // `rect.position` exactly as given rather than re-centering the box, so the drawn
            // rect's screen-space position (not just its texture sampling) shifted with `face` in
            // the old `-face * body_w`-width version -- that was the origin bug. A transform
            // sidesteps that: draw a fixed, positive-size rect centered on `c` in local space,
            // and mirror it with the canvas transform's x-axis instead (gdext has no flip_h param
            // on draw_texture_rect_ex, so this is the available lever). The source art faces LEFT,
            // so face==-1 (already left-facing) is unmirrored and face==1 mirrors, both about `c`.
            let rect = Rect2::new(
                Vector2::new(-body_w * 0.5, -body_h),
                Vector2::new(body_w, body_h),
            );
            self.base_mut()
                .draw_set_transform_matrix(Transform2D::from_cols(
                    Vector2::new(-face, 0.0),
                    Vector2::new(0.0, 1.0),
                    c,
                ));
            self.base_mut()
                .draw_texture_rect_ex(&tex, rect, false)
                .modulate(tint)
                .done();
            self.base_mut()
                .draw_set_transform_matrix(Transform2D::IDENTITY);
        } else {
            // Legs: two stubby pistons under the hips, flared foot pads at the bottom.
            let dark = Color::from_rgb(0.14, 0.16, 0.20);
            for lx in [-13.0_f32, 13.0] {
                self.base_mut().draw_rect(
                    Rect2::new(
                        c + Vector2::new(lx - leg_w * 0.5, -leg_h),
                        Vector2::new(leg_w, leg_h),
                    ),
                    dark,
                );
                self.base_mut().draw_rect(
                    Rect2::new(c + Vector2::new(lx - 11.0, -6.0), Vector2::new(22.0, 8.0)),
                    Color::from_rgb(0.20, 0.22, 0.27),
                );
            }

            // Torso: the tinted core, so the player's color still reads body-swapped.
            self.base_mut().draw_rect(
                Rect2::new(
                    c + Vector2::new(-torso_w * 0.5, torso_top),
                    Vector2::new(torso_w, torso_h),
                ),
                tint,
            );

            // Head: slate block with a cyan visor slit.
            self.base_mut().draw_rect(
                Rect2::new(
                    c + Vector2::new(-head_w * 0.5, head_top),
                    Vector2::new(head_w, head_h),
                ),
                Color::from_rgb(0.16, 0.18, 0.22),
            );
            let visor_y = head_top + head_h * 0.45;
            self.base_mut()
                .draw_line_ex(
                    c + Vector2::new(-head_w * 0.35, visor_y),
                    c + Vector2::new(head_w * 0.35, visor_y),
                    Color::from_rgb(0.30, 0.92, 1.0),
                )
                .width(4.0)
                .done();
        }

        // Shoulder pod: tinted per the rolled arm weapon, mounted on the facing side so it
        // reads as "the gun arm". A short barrel nub pokes out toward facing.
        let w = sim::ac::ArmWeapon::from_u8(f.arm);
        let pod_col = match w {
            sim::ac::ArmWeapon::MachineGun => Color::from_rgb(0.55, 0.56, 0.60),
            sim::ac::ArmWeapon::Bazooka => Color::from_rgb(0.40, 0.46, 0.22),
            sim::ac::ArmWeapon::EnergyCannon => Color::from_rgb(0.25, 0.78, 0.95),
        };
        let pod_size = Vector2::new(26.0, 24.0);
        let pod_c = c + Vector2::new(
            face * (torso_w * 0.5 + pod_size.x * 0.5 - 2.0),
            torso_top + 14.0,
        );
        self.base_mut()
            .draw_rect(Rect2::new(pod_c - pod_size * 0.5, pod_size), pod_col);
        let barrel_tip = pod_c + Vector2::new(face * 16.0, 2.0);
        self.base_mut()
            .draw_line_ex(pod_c, barrel_tip, pod_col)
            .width(7.0)
            .done();

        // Weapon label ("MG"/"BZK"/"ENG") under the pod so a player can read their roll.
        if let Some(font) = ThemeDb::singleton().get_fallback_font() {
            let label = w.name();
            let fs = 12;
            let lp = pod_c
                + Vector2::new(
                    -(label.len() as f32) * fs as f32 * 0.3,
                    pod_size.y * 0.5 + fs as f32,
                );
            self.base_mut()
                .draw_string_ex(&font, lp, label)
                .font_size(fs)
                .modulate(Color::from_rgb(0.95, 0.95, 1.0))
                .done();
        }

        // Thruster: airborne only, longer + brighter while rising (sim y-down, so vel.y < 0
        // is up). Stacked shrinking circles under each foot — same idiom as the dirt puffs.
        let airborne = matches!(
            f.state,
            CharState::Air
                | CharState::AirDodge
                | CharState::Nair
                | CharState::Fair
                | CharState::Bair
                | CharState::Uair
                | CharState::Dair
                | CharState::Helpless
        );
        if airborne {
            let rising = f.vel.y < 0.0;
            let intensity = if rising {
                (f.vel.y.abs() / 700.0).clamp(0.4, 1.0)
            } else {
                0.3
            };
            let flame_len = 16.0 + intensity * 44.0;
            let steps = 4;
            for lx in [-13.0_f32, 13.0] {
                for i in 0..steps {
                    let t0 = i as f32 / steps as f32;
                    let y = 6.0 + flame_len * t0;
                    let r = (9.0 * (1.0 - t0) + 2.0) * (0.6 + 0.4 * intensity);
                    let col = if i == 0 {
                        Color::from_rgba(1.0, 0.95, 0.55, 0.9 * intensity)
                    } else {
                        Color::from_rgba(1.0, 0.55, 0.15, (0.65 - t0 * 0.5) * intensity)
                    };
                    self.base_mut().draw_circle(c + Vector2::new(lx, y), r, col);
                }
            }
        }
    }
}
