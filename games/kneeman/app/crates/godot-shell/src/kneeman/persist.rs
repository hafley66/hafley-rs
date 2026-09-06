//! Durable world persistence: settled-ink diffing into the event log (A4), autosave
//! cadence, and the save-slot API surfaced by the pause menu.

use crate::net::now_ms;
use crate::sim::{self};

use super::{KneeMan, Phase};

impl KneeMan {
    /// A4: mirror the sim's settled drawn ink — pencil AND pen — into the durable world log.
    /// Runs offline-only on a ~0.5s cadence: diff the settled strokes against `ink_base`
    /// (content-keyed), append `AddStroke` for new ones and `Revert` for ones knocked away, moved,
    /// or decayed (a pencil stroke's expiry IS a disappearance in this diff)
    /// (a moved stroke is revert + re-add — forward events, history kept). Points are RDP-simplified
    /// (bends only; segments interpolate) then snapped to a 1/8px grid so a rehydrate → re-diff
    /// round-trip is byte-stable and boot never churns the log. Netplay persistence waits on the
    /// confirmed-frame bridge (world/bridge.rs) — mispredicted frames must never become facts.
    // parity(v1-ink-durable-world-diff): offline settled ink becomes quantized add and revert world events, while drawing, traveling, baked, and rollback-unconfirmed states never become durable facts
    pub(super) fn persist_ink(&mut self) {
        if self.phase != Phase::Offline || self.ev_tick % 30 != 0 {
            return;
        }
        let Some(rt) = self.world.as_mut() else {
            return;
        };
        let me = rt.owner();
        let s = self.state.get();
        let quant = |v: sim::Vector2| {
            sim::Vector2::new((v.x * 8.0).round() / 8.0, (v.y * 8.0).round() / 8.0)
        };
        let mut cur: std::collections::BTreeMap<Vec<u8>, (sim::StrokeId, Vec<sim::Vector2>)> =
            std::collections::BTreeMap::new();
        for p in s.paths.iter() {
            // Durable Stroke records cannot represent cell identity or damage yet.
            // Full SimState snapshots preserve both; keep cells session-local here.
            if !p.active() || p.drawing || p.traveling() || p.mass <= 0.0 || p.cell.is_some() {
                continue; // only settled bodies are durable facts (baked stage = mass 0, skipped)
            }
            // ALL drawing is world-visible, pencil included: an expiring stroke persists on settle
            // and its later decay shows up as a disappearance in this same diff -> Revert. So the
            // log always mirrors what's actually standing, and a joiner sees live pencil ink too.
            let world: Vec<sim::Vector2> = (0..p.len as usize)
                .map(|i| p.world_pt(i, &s.nodes))
                .collect();
            let pts: Vec<sim::Vector2> = sim::simplify_polyline(&world, 2.0)
                .into_iter()
                .map(quant)
                .collect();
            // sim strokes don't know their registry row; recover it by matching props (row order
            // stable, tiny table). No match (props panel-edited mid-flight): a permanent stroke
            // falls back to the tetris row (a row-0 fallback would rehydrate it as expiring
            // pencil, a silent delete on next boot); an expiring one falls back to row 0.
            let fallback = if p.props.stroke_life < 0 {
                sim::StrokeRegistry::TETRIS_ROW as usize
            } else {
                0
            };
            let stroke = self
                .tune
                .get_cloned()
                .strokes
                .presets
                .iter()
                .position(|pr| *pr == p.props)
                .unwrap_or(fallback) as sim::StrokeId;
            cur.insert(sim::world::canon(&(stroke, pts.clone())), (stroke, pts));
        }
        // additions: settled permanent ink not yet in the log
        for (k, (stroke, pts)) in &cur {
            if !self.ink_base.contains_key(k) {
                let id = rt.append(&sim::world::WorldEvent::AddStroke {
                    owner: me,
                    stroke: *stroke,
                    pts: pts.clone(),
                });
                self.ink_base.insert(k.clone(), id);
            }
        }
        // removals: persisted strokes that no longer exist at that shape/place (struck away, or
        // mid-flight right now — a traveling stroke reverts here and re-adds when it settles)
        let stale: Vec<Vec<u8>> = self
            .ink_base
            .keys()
            .filter(|k| !cur.contains_key(*k))
            .cloned()
            .collect();
        for k in stale {
            if let Some(placed) = self.ink_base.remove(&k) {
                rt.append(&sim::world::WorldEvent::Revert { target: placed });
            }
        }
    }

    /// Per-frame auto-save of the durable home. Every minute of session time the world runtime
    /// bookmarks the current head; a return value means it just saved, and we warn if the cache is
    /// over the soft cap (mostly gif blobs — nothing is auto-deleted).
    pub(super) fn tick_autosave(&mut self, dt: f32) {
        let Some(w) = self.world.as_mut() else { return };
        if let Some(bytes) = w.autosave(dt, now_ms()) {
            if bytes > crate::world_runtime::CACHE_WARN_BYTES {
                let mb = bytes / (1024 * 1024);
                crate::toast::push(
                    &self.toasts,
                    crate::toast::ToastKind::Warn,
                    &format!("World cache is {mb} MB — delete old saves to free space"),
                );
            }
        }
    }

    // --- durable-world CRUD, driven by the debug panel's Saves tab (it holds a Gd<KneeMan>) ---

    /// Every save for the home world, oldest first (empty if the world failed to boot).
    pub fn world_slots(&self) -> Vec<crate::sim::world::store::Slot> {
        self.world.as_ref().map(|w| w.slots()).unwrap_or_default()
    }

    /// Rough stored footprint of the home world, in bytes.
    pub fn world_cache_bytes(&self) -> u64 {
        self.world.as_ref().map(|w| w.cache_bytes()).unwrap_or(0)
    }

    /// Save the current home under `label` (overwrites a same-label save).
    pub fn world_save(&mut self, label: &str) {
        if let Some(w) = self.world.as_mut() {
            w.save_slot(label, now_ms());
            crate::toast::push(
                &self.toasts,
                crate::toast::ToastKind::Success,
                &format!("Saved “{label}”"),
            );
        }
    }

    /// Restore a saved home by label.
    pub fn world_load(&mut self, label: &str) {
        if let Some(w) = self.world.as_mut() {
            w.load_slot(label);
            crate::toast::push(
                &self.toasts,
                crate::toast::ToastKind::Info,
                &format!("Loaded “{label}”"),
            );
        }
    }

    /// Forget a save by label.
    pub fn world_delete(&mut self, label: &str) {
        if let Some(w) = self.world.as_mut() {
            w.delete_slot(label);
        }
    }

    /// Reset the home to the blank static defaults (undo-able forward event).
    pub fn world_reset(&mut self) {
        if let Some(w) = self.world.as_mut() {
            w.reset_home();
            self.ink_base.clear();
            // wipe the LIVE drawn ink too — without this the persist diff re-appends every stroke
            // still standing in the sim ~0.5s later and the reset undoes itself (the "lines won't
            // go away" loop). Baked stage strokes (owner < 0) stay.
            let mut s = self.state.get();
            for p in s.paths.iter_mut() {
                if p.owner >= 0 {
                    *p = sim::InkPath::EMPTY;
                }
            }
            self.state.set(s);
            crate::toast::push(
                &self.toasts,
                crate::toast::ToastKind::Info,
                "Home reset to defaults",
            );
        }
    }
}
