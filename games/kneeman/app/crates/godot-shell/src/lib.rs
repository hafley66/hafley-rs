use godot::prelude::*;

mod analytics; // netcode event firehose: buffer -> POST /ev (rotating log on the relay)
use kneeman_core::asset_wire;
mod controls; // sole device->InputFrame boundary (GameAction universe)
mod gif; // gif background: pure decode + game-time frame clock (plans/gif-background-library.md, G1)
mod gif_lib; // gif library storage over an injected root dir (plans/gif-background-library.md, G2)
mod godot_store; // WorldStore over user:// (disk native / IndexedDB web); the client's durable backend
mod grid; // training-room grid backdrop
mod identity; // local player identity: name/color + per-slot defaults + user://identity.cfg persistence
use kneeman_core::input_frame;
mod kneeman; // impure shell: input -> step -> publish -> render
mod net; // stateless netplay support: snapshot codec, room codes, transport-state names, NetDebug DTO
use kneeman_core::netplay;
mod roster; // character roster: built-ins + assets/roster.json loader (pure data, lifted from kneeman)
mod rtc; // Godot WebRTC netplay transport (ggrs over a browser data channel)
mod scripted; // ScriptedDevice: tick-keyed input script substitution for the local-input source (plans/e2e-playwright.md)
mod shared_camera; // shared render-only framing for the V1 and V4 shell adapters
mod sprite; // sprite/label render helpers: tint, tags, AnimatedSprite2D clip + SpriteFrames machinery
mod toast; // global snackbar: Mutable<Vec<Toast>> cell, emitted on phase changes, drawn over everything
mod ui; // egui debug panel + XP menu nav system + themes (builds on wasm via patched gdext-egui)
mod webtest; // web test hooks: ?autofind/?script boot params + window.__smash (gated, web builds only; plans/e2e-playwright.md)
mod world_runtime; // durable-world shell adapter: load/save home world + gif background over any WorldStore

// Simulation and rollback live in the shared Kneeman game crate. Local aliases preserve
// the shell's existing call sites. `gv()` is the glam-to-Godot vector boundary.
use kneeman_core as v1;
use v1 as sim;

#[cfg(test)]
#[path = "../../../content/rust/0_ship.rs"]
mod ship_receipt;

// Extension entry (referenced by sim.gdextension as gdext_rust_init).
// GodotClass types register themselves wherever they live; the modules above just need compiling.
struct SmashSimExtension;

#[gdextension]
unsafe impl ExtensionLibrary for SmashSimExtension {}
