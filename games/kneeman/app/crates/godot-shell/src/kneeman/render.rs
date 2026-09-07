//! Per-frame render pass: camera tracking, fighter sprites, nametags, edge chips,
//! and the bottom damage HUD. Pure SimState reads; no sim writes.

use godot::classes::{Texture2D, ThemeDb};
use godot::prelude::*;
use godot::tools::try_load;

use crate::identity::{save_identity, slot_color, slot_name};
use crate::roster::roster;
use crate::sim::{self, Fighter, SimState, Tune};
use crate::sprite::{
    apply_character, clip_for, impact_pop, place_tag, resolve_clip, sprite_tint, sync_attack_frame,
    wall_tilt,
};

use super::{KneeMan, fx_jitter, gv};

/// Command-grab fireball texture. Optional and gitignored (private art under `assets/`); when the
/// file is absent `draw_fire` falls back to a procedural fireball. Same load-or-fallback shape as
/// `MECH_TEX`, just loaded on demand instead of cached.
const FIRE_TEX: &str = "res://assets/fx/fire.png";

impl KneeMan {
    /// The blast frame as VISIBLE hard red walls (2026-07-04 director's call): the same
    /// `BLAST_*` rect the KO check and the hull's wall bounce read, drawn as four translucent
    /// red lines behind everything else -- the ship visibly caroms off exactly the line
    /// painted here, and fighters die crossing it, so the boundary is one honest rectangle.
    pub(super) fn draw_blast_walls(&mut self, origin: Vector2) {
        let lo = gv(sim::Vector2::new(sim::BLAST_LEFT, sim::BLAST_TOP)) - origin;
        let hi = gv(sim::Vector2::new(sim::BLAST_RIGHT, sim::BLAST_Y)) - origin;
        let red = Color::from_rgba(1.0, 0.18, 0.16, 0.65);
        let corners = [
            Vector2::new(lo.x, lo.y),
            Vector2::new(hi.x, lo.y),
            Vector2::new(hi.x, hi.y),
            Vector2::new(lo.x, hi.y),
        ];
        for i in 0..4 {
            self.base_mut()
                .draw_line_ex(corners[i], corners[(i + 1) % 4], red)
                .width(6.0)
                .done();
        }
    }

    /// One-way gate ticks for one ink segment: a gated stroke (`gate_side != Off`) shows WHICH
    /// side passes -- short bright ticks pointing along the oriented pass normal
    /// (`segment_gate_normal` of the RAW drawing-order endpoints, sign-flipped for PassBackward
    /// -- the same axis + sign `gate_admits` tests, so the drawing IS the collision truth).
    /// Bodies moving WITH the ticks pass; against them, blocked. No-op for ungated/mid-draw
    /// strokes and None segments.
    // parity(v1-ink-valve-presentation): live one-way segments draw pass-direction ticks from the same ordered endpoint normal and gate side used by collision admission
    pub(super) fn draw_gate_ticks(
        &mut self,
        p: &sim::InkPath,
        seg: usize,
        origin: Vector2,
        jit: Vector2,
        nodes: &[sim::InkNode],
    ) {
        if p.drawing
            || p.props.gate_side == sim::GateSide::Off
            || p.seg_class(seg, nodes) == sim::SegClass::None
        {
            return;
        }
        let (wa, wb) = (p.world_pt(seg, nodes), p.world_pt(seg + 1, nodes));
        let raw = sim::body::contact::segment_gate_normal(wa, wb);
        let pass = if p.props.gate_side == sim::GateSide::PassForward {
            raw
        } else {
            -raw
        };
        for frac in [0.3_f32, 0.7] {
            let base = wa + (wb - wa) * frac;
            let tip = base + pass * 14.0;
            self.base_mut()
                .draw_line_ex(
                    gv(base) - origin + jit,
                    gv(tip) - origin + jit,
                    Color::from_rgb(0.35, 1.0, 0.55),
                )
                .width(3.0)
                .done();
        }
    }

    /// One ink segment, drawn with a two-tone body for SOLID strokes: the class color OUTSIDE and
    /// PURPLE INSIDE, so a solid container reads as "you are on the purple side". Two half-width
    /// polylines offset +/- a quarter width along the segment's oriented normal
    /// (`segment_gate_normal`, the same drawing-order seam the gate ticks ride, flipped to point
    /// away from the stroke center so the inside of a closed hull is the purple half). GATE strokes
    /// are skipped -- their green pass-ticks already mark the passable side, and a two-tone body
    /// would fight that read -- as are live (mid-draw) and None-class segments; those fall back to a
    /// single plain line, so the caller swaps exactly one `draw_line_ex` for one call here.
    // parity(v1-ink-solid-presentation): finalized solid and ledge segments split their stroke across the collision normal so class, grabbable top, and contained side remain visible
    pub(super) fn draw_ink_segment(
        &mut self,
        p: &sim::InkPath,
        seg: usize,
        a: Vector2,
        b: Vector2,
        col: Color,
        width: f32,
        nodes: &[sim::InkNode],
    ) {
        // A LEDGE-classed segment gets the two-half styling too, its TOP (outward) half YELLOW --
        // the grabbable tell -- composing with the purple-inside rule: a solid ledge = yellow top +
        // purple inner. A non-solid (only-ledge) segment shows yellow over its own base color.
        let ledge = p.seg_class(seg, nodes) == sim::SegClass::Ledge;
        let two_tone = (p.props.solid || ledge)
            && p.props.gate_side == sim::GateSide::Off
            && !p.drawing
            && p.seg_class(seg, nodes) != sim::SegClass::None;
        if !two_tone {
            self.base_mut().draw_line_ex(a, b, col).width(width).done();
            return;
        }
        let (wa, wb) = (p.world_pt(seg, nodes), p.world_pt(seg + 1, nodes));
        let raw = sim::body::contact::segment_gate_normal(wa, wb);
        // point the normal OUTWARD (away from the stroke center `p.pos`); `-outward` is the inside.
        let mid = (wa + wb) * 0.5;
        let outward = if raw.dot(mid - p.pos) < 0.0 {
            -raw
        } else {
            raw
        };
        let off = gv(outward) * (width * 0.25); // gv is identity coords, so world normal == draw normal
        let half = width * 0.5;
        // outward half: yellow for a ledge tell, else the class color. inner half: purple for a
        // solid stroke's inside, else the base color (a non-solid ledge's bottom stays plain).
        let top = if ledge {
            Color::from_rgba(0.95, 0.85, 0.30, col.a)
        } else {
            col
        };
        let inner = if p.props.solid {
            Color::from_rgba(0.62, 0.35, 0.9, col.a) // inside hue at the stroke's own alpha
        } else {
            col
        };
        self.base_mut()
            .draw_line_ex(a + off, b + off, top)
            .width(half)
            .done();
        self.base_mut()
            .draw_line_ex(a - off, b - off, inner)
            .width(half)
            .done();
    }

    /// Bottom damage panel: name + % per fighter, P1 anchored bottom-left, P2 bottom-right. The %
    /// tints from white toward red as damage climbs (the "about to die" read).
    /// Track the camera to keep both fighters framed: center on their midpoint, zoom out so the
    /// pair (plus margin) fits the viewport, clamp loosely to the stage, and ease toward the target
    /// so it glides instead of snapping. Mirrors melee/PM "shared camera". Render-only (no sim state).
    pub(super) fn update_camera(&mut self) {
        let Some(mut cam) = self.cam.clone() else {
            return;
        };
        let s = self.state.get();
        let active = (s.active as usize).max(1);
        let subjects = s.fighters[..active]
            .iter()
            .map(|fighter| gv(fighter.pos))
            .collect::<Vec<_>>();
        let local = subjects[self.local_handle.min(active - 1)];
        let view = self.base().get_viewport_rect().size;
        let zone = sim::zone::ZoneRect::extended(sim::ink_blast_zone(&s.paths, &s.nodes));
        crate::shared_camera::update_shared_camera(
            &mut cam,
            view,
            &subjects,
            local,
            gv(zone.lo),
            gv(zone.hi),
        );
    }

    pub(super) fn update_hud(&mut self) {
        let s = self.state.get();
        let active = s.active as usize;
        let view = self.base().get_viewport_rect().size;
        // Lay the panels out across the bottom strip, anchored by CONTENT edge: slot 0's left edge
        // at the left inset, the last slot's RIGHT edge at the right inset (long names grow inward
        // instead of running off-screen), the rest spaced between. Dormant slots (>= active) hidden.
        const INSET: f32 = 70.0;
        let y = view.y - 150.0;
        for k in 0..sim::MAX_PLAYERS {
            let Some(mut l) = self.hud[k].clone() else {
                continue;
            };
            if k >= active {
                l.set_visible(false);
                continue;
            }
            l.set_visible(true);
            let name = self.player_name(k);
            let pct = s.fighters[k].damage.round() as i32;
            l.set_text(&format!("{name}\n{pct}%"));
            let danger = (s.fighters[k].damage / 150.0).clamp(0.0, 1.0);
            l.add_theme_color_override(
                "font_color",
                Color::from_rgb(1.0, 1.0 - danger, 1.0 - danger),
            );
            l.reset_size(); // shrink-wrap to the new text before measuring
            let w = l.get_size().x;
            let t = if active <= 1 {
                0.0
            } else {
                k as f32 / (active - 1) as f32
            };
            // slides the anchor from left edge (t=0) to right edge (t=1), width-aware.
            let x = INSET + t * (view.x - 2.0 * INSET - w);
            l.set_position(Vector2::new(x, y));
        }
    }

    /// Nametag/HUD text for fighter `idx`. In netplay the local fighter is `local_handle` (0 for
    /// host, 1 for guest), not always slot 0. The remote slot uses `peer_identity` if received, else
    /// falls back to the generic "Pn" label. Cosmetic only.
    pub(super) fn player_name(&self, idx: usize) -> String {
        if idx == self.local_handle {
            self.identity.get_cloned().name
        } else if idx + self.local_handle == 1 {
            self.peer_identity
                .as_ref()
                .map(|id| id.name.clone())
                .unwrap_or_else(|| slot_name(idx))
        } else {
            slot_name(idx)
        }
    }

    /// Slot color for fighter `idx`. Local fighter wears the live identity color; the remote slot
    /// wears `peer_identity.color` if received, else the fixed slot palette. Cosmetic only.
    pub(super) fn slot_tint(&self, idx: usize) -> Color {
        if idx == self.local_handle {
            self.identity.get_cloned().color
        } else if idx + self.local_handle == 1 {
            self.peer_identity
                .as_ref()
                .map(|id| id.color)
                .unwrap_or_else(|| slot_color(idx))
        } else {
            slot_color(idx)
        }
    }

    /// Drive every live fighter's sprite, hiding the dormant slots (>= active). Slot 0 is the node's
    /// own child, so it tracks the node position; slots 1.. are world-space siblings positioned here.
    pub(super) fn render_fighters(&mut self, s: &SimState) {
        let active = s.active as usize;
        for k in 0..active {
            self.render_fighter(k, &s.fighters[k]);
        }
        for k in active..sim::MAX_PLAYERS {
            if let Some(mut a) = self.sprites[k].clone() {
                a.set_visible(false);
            }
        }
    }

    /// Drive one fighter's sprite: clip for the state, flip by facing, slot tint, and (for the
    /// world-space siblings, slot != 0) the feet position. Green tint while intangible — the
    /// universal "you can't be hit" read.
    ///
    /// An Armored Core fighter has no sprite: the body is replaced by the boxy mech `draw()`
    /// paints each frame (see `mod.rs`). Hide the AnimatedSprite2D and skip the clip/tint work
    /// entirely; tags and HUD are driven elsewhere and keep working untouched.
    pub(super) fn render_fighter(&mut self, idx: usize, f: &Fighter) {
        let Some(mut a) = self.sprites[idx].clone() else {
            return;
        };
        if f.has_badge(sim::Badge::AcCore) {
            a.set_visible(false);
            return;
        }
        a.set_visible(true);
        if idx != 0 {
            a.set_global_position(gv(f.pos)); // slot 0 tracks the node; siblings position here
        }
        let clip = resolve_clip(&a, clip_for(f));
        if a.get_animation() != clip {
            a.play_ex().name(&clip).done(); // only restart when the clip actually changes
        }
        sync_attack_frame(&mut a, f, &self.tune.get_cloned());
        a.set_flip_h(f.facing < 0.0); // frog faces right by default
        a.set_rotation(wall_tilt(f));
        a.set_scale(Vector2::splat(self.base_scale[idx] * impact_pop(f))); // squash-pop on a connect
        a.set_modulate(sprite_tint(f, self.slot_tint(idx)));
    }

    /// Persist the identity when the panel changes it, and refresh P1's nametag to match.
    pub(super) fn sync_identity(&mut self) {
        let id = self.identity.get_cloned();
        if id == self.saved_identity {
            return;
        }
        save_identity(&id);
        // Local fighter's tag wears the live name + color; every tag shares the local player's font size.
        let local = self.local_handle;
        for (k, tag) in self.tags.iter().enumerate() {
            let Some(mut tag) = tag.clone() else { continue };
            if k == local {
                tag.set_text(&id.name);
                tag.add_theme_color_override("font_color", id.color);
            }
            tag.add_theme_font_size_override("font_size", id.font_px);
        }
        self.saved_identity = id;
    }

    /// Character flow, both directions. `char_id` is SIM state now (rolls back, checksums,
    /// respawn keeps it), so this is a two-step sync each frame:
    ///   menu -> sim: offline, the charsel picks write straight into the fighters. In a net
    ///   match the sim is authoritative + rolled back, so picks were stamped once at
    ///   `begin_session` (both peers stamp the same values off the handshake) and the menu
    ///   can't touch them mid-match.
    ///   sim -> sprites: paint whatever the WORLD says each fighter is. An in-map character
    ///   switch that mutates `char_id` inside `step` renders with zero shell wiring.
    pub(super) fn sync_charsel(&mut self) {
        let roster = roster();
        let clamp = |v: i64| (v.max(0) as usize).min(roster.len() - 1);
        let want = self.charsel.get_cloned();
        // persist the raw picks next to name/color, so the character survives a relaunch
        if want != self.saved_charsel {
            crate::identity::save_charsel(want);
            self.saved_charsel = want;
        }

        if self.net.is_none() {
            let mut s = self.state.get();
            let mut dirty = false;
            for slot in 0..want.len().min(s.fighters.len()) {
                let id = clamp(want[slot]) as u8;
                if s.fighters[slot].char_id != id {
                    s.fighters[slot].char_id = id;
                    dirty = true;
                }
            }
            if dirty {
                self.state.set(s);
            }
        }

        let s = self.state.get();
        for slot in 0..self.characters.len() {
            let id = (s.fighters[slot].char_id as usize).min(roster.len() - 1);
            if id == self.characters[slot] {
                continue;
            }
            self.characters[slot] = id;
            let c = &roster[id];
            if let Some(mut a) = self.sprites[slot].clone() {
                apply_character(&mut a, c);
                self.base_scale[slot] = c.scale;
            }
        }
    }

    /// Hover each live fighter's nametag a fixed height over its head; hide the dormant slots.
    /// Refreshes text and color every frame so the peer's tag shows their name+color as soon as
    /// `peer_identity` arrives from the SDP handshake (no separate change-detection needed).
    pub(super) fn place_tags(&mut self) {
        let s = self.state.get();
        let active = s.active as usize;
        for k in 0..sim::MAX_PLAYERS {
            let Some(mut tag) = self.tags[k].clone() else {
                continue;
            };
            if k >= active {
                tag.set_visible(false);
                continue;
            }
            tag.set_visible(true);
            tag.set_text(&self.player_name(k));
            tag.add_theme_color_override("font_color", self.slot_tint(k));
            place_tag(&mut tag, s.fighters[k].pos);
        }
        self.place_edge_tags();
    }

    /// Show a screen-edge chip for any fighter launched out of view: name + a pointer arrow toward
    /// them + the off-screen distance. Hidden while the fighter is on-screen (the world nametag
    /// covers that case). Uses the live camera transform to map world feet -> screen pixels.
    pub(super) fn place_edge_tags(&mut self) {
        let Some(cam) = self.cam.clone() else { return };
        let s = self.state.get();
        let view = self.base().get_viewport_rect().size;
        let cam_c = cam.get_position();
        let zoom = cam.get_zoom();
        let active = s.active as usize;
        let names: Vec<String> = (0..sim::MAX_PLAYERS).map(|k| self.player_name(k)).collect();
        const M: f32 = 56.0; // keep the chip this far inside the screen edge
        for k in 0..sim::MAX_PLAYERS {
            let Some(tag) = self.edge_tags[k].as_mut() else {
                continue;
            };
            if k >= active {
                tag.set_visible(false);
                continue;
            }
            let world = gv(s.fighters[k].pos);
            let screen = (world - cam_c) * zoom + view * 0.5;
            let off =
                screen.x < M || screen.x > view.x - M || screen.y < M || screen.y > view.y - M;
            if !off {
                tag.set_visible(false);
                continue;
            }
            // dominant off-screen direction picks the arrow; distance is the raw off-stage pixels.
            let dx = if screen.x < M {
                M - screen.x
            } else if screen.x > view.x - M {
                screen.x - (view.x - M)
            } else {
                0.0
            };
            let dy = if screen.y < M {
                M - screen.y
            } else if screen.y > view.y - M {
                screen.y - (view.y - M)
            } else {
                0.0
            };
            let arrow = if dy >= dx {
                if screen.y < M { "▲" } else { "▼" }
            } else if screen.x < M {
                "◀"
            } else {
                "▶"
            };
            let dist = dx.max(dy).round() as i32;
            tag.set_text(&format!("{arrow} {} {dist}", names[k]));
            tag.set_visible(true);
            let sz = tag.get_size();
            let px = (screen.x - sz.x * 0.5).clamp(M, view.x - M - sz.x);
            let py = (screen.y - sz.y * 0.5).clamp(M, view.y - M - sz.y);
            tag.set_position(Vector2::new(px, py));
        }
    }

    /// Pickup tooltip: the LOCAL fighter only (this player's own screen), so a nearby item
    /// pops a popover with its name + how-to-use. Same reach predicate the sim itself uses
    /// to grant the pickup (`nearest_pickup`), so the tooltip never lies about being in
    /// range. Falls back to skipping when a matched kind has no MENU_ITEMS row (shouldn't
    /// happen: every held-tool/badge kind nearest_pickup can match has one).
    ///
    /// Anchored above the LOCAL FIGHTER's own head (not the item, and not the feet/item-level
    /// spot it used to sit at, which a standing crowd covered): clears the ECB top (body height,
    /// `sim::ECB_HALF_H * 2`) plus `HEAD_CLEARANCE` so it also floats above the nametag riding
    /// just over the head (`sprite::place_tag`'s TAG_RISE). World-space-clamped against
    /// `sim::BLAST_TOP` (same lever `update_camera` uses) so it stays fully on-screen even when
    /// the fighter is launched up near the top blast line.
    pub(super) fn draw_pickup_tooltip(
        &mut self,
        s: &SimState,
        t: &Tune,
        active: usize,
        origin: Vector2,
    ) {
        let local = self.local_handle.min(active.saturating_sub(1));
        let lf = s.fighters[local];
        // Three tooltip sources, most specific first: seated at the helm (the piloting
        // controls), a mounted station in interact reach (how to take it -- `nearest_pickup`
        // never matches stations, they are neither held tools nor badges), then an ordinary
        // pickup's MENU_ITEMS card.
        let helm: Option<(&str, &str)> = if lf.station >= 0 {
            Some(("Helm", "c-stick: aim + hold attack: burn + jump: bail"))
        } else if sim::nearest_station(&lf, &s.items, t).is_some() {
            Some(("Helm", "attack: take the helm and fly the ship"))
        } else {
            None
        };
        let pickup: Option<(&str, &str)> = sim::nearest_pickup(&lf, &s.items, t).and_then(|idx| {
            sim::MENU_ITEMS
                .iter()
                .find(|c| c.kind == s.items[idx].kind)
                .map(|c| (c.name, c.how))
        });
        if let Some((name, how)) = helm.or(pickup) {
            {
                const HEAD_CLEARANCE: f32 = 40.0; // above the ECB top, clear of the nametag too
                let name_fs = 15;
                let how_fs = 12;
                let name_w = name.len() as f32 * name_fs as f32 * 0.56;
                let how_w = how.len() as f32 * how_fs as f32 * 0.5;
                let box_w = name_w.max(how_w) + 18.0;
                let box_h = name_fs as f32 + how_fs as f32 + 16.0;
                let head_y = (lf.pos.y - sim::ECB_HALF_H * 2.0 - HEAD_CLEARANCE)
                    .max(sim::BLAST_TOP + box_h + 24.0); // keep the whole box shy of the blast line
                let anchor = gv(sim::Vector2::new(lf.pos.x, head_y)) - origin;
                let box_tl = anchor - Vector2::new(box_w * 0.5, box_h + 8.0);
                let bg = Color::from_rgba(0.05, 0.07, 0.11, 0.82);
                self.base_mut()
                    .draw_rect(Rect2::new(box_tl, Vector2::new(box_w, box_h)), bg);
                // pointer triangle: box bottom-center down toward the fighter's head.
                let base_l = anchor + Vector2::new(-8.0, -8.0);
                let base_r = anchor + Vector2::new(8.0, -8.0);
                self.base_mut()
                    .draw_line_ex(base_l, anchor, bg)
                    .width(3.0)
                    .done();
                self.base_mut()
                    .draw_line_ex(base_r, anchor, bg)
                    .width(3.0)
                    .done();
                self.base_mut()
                    .draw_line_ex(base_l, base_r, bg)
                    .width(3.0)
                    .done();
                if let Some(font) = ThemeDb::singleton().get_fallback_font() {
                    let name_pos = box_tl + Vector2::new(9.0, name_fs as f32 + 1.0);
                    // faux-bold: draw the name twice, offset by a pixel (no bold font
                    // resource wired up -- same ThemeDb fallback-only trick `draw_mech` uses).
                    for off in [Vector2::ZERO, Vector2::new(1.0, 0.0)] {
                        self.base_mut()
                            .draw_string_ex(&font, name_pos + off, name)
                            .font_size(name_fs)
                            .modulate(Color::from_rgb(1.0, 1.0, 1.0))
                            .done();
                    }
                    let how_pos = name_pos + Vector2::new(0.0, how_fs as f32 + 4.0);
                    self.base_mut()
                        .draw_string_ex(&font, how_pos, how)
                        .font_size(how_fs)
                        .modulate(Color::from_rgba(0.82, 0.88, 0.95, 0.95))
                        .done();
                }
            }
        }
    }

    /// Falcon command-grab boom: a deliberately cheesy fireball. Tries a texture at `FIRE_TEX`
    /// with the exact load-or-fallback shape `draw_mech` uses for `MECH_TEX` (`try_load(...).ok()`,
    /// draw the sprite if it loaded). When the file is missing -- it is gitignored by default -- it
    /// paints a shitty-youtube-effect procedural fireball instead: stacked orange/yellow/white discs
    /// with jagged flame spikes, the whole thing scaling up over the fx's life. `c` is the node-local
    /// center, `age` frames since the boom, `tick` the fx spawn tick (stable jitter seed).
    pub(super) fn draw_fire(&mut self, c: Vector2, age: f32, tick: u64) {
        let life = 28.0_f32;
        if age >= life {
            return;
        }
        let a = age / life;
        let grow = 1.0 + a * 2.4; // scales up over its lifetime -- bigger = cheesier
        let alpha = (1.0 - a).powf(1.1);
        let texture = if godot::classes::ResourceLoader::singleton().exists(FIRE_TEX) {
            try_load::<Texture2D>(FIRE_TEX).ok()
        } else { None };
        if let Some(tex) = texture {
            let (tw, th) = (tex.get_width() as f32, tex.get_height() as f32);
            let dw = 96.0 * grow;
            let dh = if tw > 0.0 { dw * (th / tw) } else { dw };
            let rect = Rect2::new(c - Vector2::new(dw * 0.5, dh * 0.5), Vector2::new(dw, dh));
            self.base_mut()
                .draw_texture_rect_ex(&tex, rect, false)
                .modulate(Color::from_rgba(1.0, 1.0, 1.0, alpha))
                .done();
            return;
        }
        // Fallback: layered discs + jagged spikes. Deliberately garish.
        let r = 24.0 * grow;
        for spike in 0..10u64 {
            let jit = fx_jitter(tick, spike);
            let ang = spike as f32 / 10.0 * std::f32::consts::TAU + jit * 0.7;
            let dir = Vector2::new(ang.cos(), ang.sin());
            let len = r * (1.1 + jit * 1.2);
            self.base_mut()
                .draw_line_ex(
                    c + dir * r * 0.35,
                    c + dir * len,
                    Color::from_rgba(1.0, 0.45 + jit * 0.25, 0.05, alpha),
                )
                .width(4.0)
                .done();
        }
        self.base_mut()
            .draw_circle(c, r, Color::from_rgba(1.0, 0.30, 0.02, alpha * 0.5));
        self.base_mut()
            .draw_circle(c, r * 0.66, Color::from_rgba(1.0, 0.55, 0.05, alpha * 0.78));
        self.base_mut()
            .draw_circle(c, r * 0.38, Color::from_rgba(1.0, 0.88, 0.28, alpha * 0.9));
        self.base_mut()
            .draw_circle(c, r * 0.16, Color::from_rgba(1.0, 1.0, 0.92, alpha));
    }
}
