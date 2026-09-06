//! Shell adapter for the durable world (`crate::sim::world`). Owns the sqlite store plus the current
//! world id/head, so the rest of the shell says "load my home", "set the background", "what does the
//! world look like" without ever touching event ids. This is the seam the sim writes durable facts
//! through (Stage 1). Save/load is just: `boot` (open + load-or-create) and `world` (fold the log).

use crate::sim::Tune;
use crate::sim::world::fold::{World, fold};
use crate::sim::world::store::{Slot, WorldStore};
use crate::sim::world::{
    AssetErr, AssetId, BuildVersion, EventId, PlayerId, Seed, Seq, StageId, WorldEvent, WorldId,
};

/// Genesis build of a home world. Bumping it is a namespace split (new `WorldId`), so it is pinned.
const HOME_BUILD: BuildVersion = BuildVersion(1);

/// Auto-save cadence: snapshot the current head every minute of session time.
pub const AUTOSAVE_SECS: f32 = 60.0;
/// Auto-save labels share this prefix; only these get pruned (manual saves are never evicted).
const AUTO_PREFIX: &str = "auto ";
/// Keep at most this many auto-saves (a ring); older ones are dropped.
const MAX_AUTOSAVES: usize = 10;
/// Soft cache cap. Crossing it (mostly gif blobs) raises a warn toast; nothing is deleted.
pub const CACHE_WARN_BYTES: u64 = 32 * 1024 * 1024;

/// The genesis marker for a reset: `Reset { to }` with a target not in the log folds back to genesis
/// (fold.rs), and an all-zero id is the None-parent sentinel — never a real event id, so always "unknown".
const GENESIS: EventId = EventId([0u8; 32]);

/// Where a rendered background's pixels come from, right now. Three states because "set" and
/// "held locally" are independent: a rejoining peer folds a `SetBackground` naming an `AssetId`
/// before the P2P pull (task W2) has landed the blob.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BgSource {
    /// No background ever set (or cleared back to it) — render the stage's own backdrop.
    None,
    /// Set, and the blob is held locally — render these pixels.
    Bytes(Vec<u8>),
    /// Set, but the blob isn't held locally yet. Carries the id so the caller can queue a Want.
    Missing(AssetId),
}

/// Generic over the storage backend so the client mounts `GodotStore` (user:// = disk/IndexedDB) and
/// the server mounts `SqliteStore`, with everything above the store unchanged.
pub struct WorldRuntime<S: WorldStore> {
    store: S,
    current: WorldId, // the loaded world (a player's home, for now)
    head: Option<EventId>,
    owner: PlayerId,
    auto_accum: f32, // seconds of session time toward the next auto-save
}

impl<S: WorldStore> WorldRuntime<S> {
    /// Load-or-create the caller's home world on the given backend. `owner` is the player's persistent
    /// key (KneeMan supplies + persists it; it distinguishes one home from another). Idempotent: same
    /// owner -> same home, re-attached.
    pub fn boot(mut store: S, owner: PlayerId) -> Self {
        // publish is idempotent (re-attach on disk / INSERT OR IGNORE): create once, re-open after.
        let current = store.publish(&home_seed(owner));
        let head = store.head(current);
        WorldRuntime {
            store,
            current,
            head,
            owner,
            auto_accum: 0.0,
        }
    }

    pub fn owner(&self) -> PlayerId {
        self.owner
    }
    pub fn world_id(&self) -> WorldId {
        self.current
    }
    pub fn head(&self) -> Option<EventId> {
        self.head
    }

    /// Fold the current world's whole log into live state (save/load: this IS load).
    pub fn world(&self) -> World {
        fold(HOME_BUILD, &self.store.since(self.current, None))
    }

    /// Append one event to the current world, advancing head. The single durable-write path — the
    /// bridge (confirmed geometry) and presence both funnel here.
    pub fn append(&mut self, ev: &WorldEvent) -> EventId {
        let id = self.store.append(self.current, ev);
        self.head = Some(id);
        id
    }

    /// Owner sets/changes the gif background: stash the bytes as a content-addressed asset (blob out of
    /// the event log), then record the pointer as a folded `SetBackground`. Returns the `AssetId`, or
    /// `AssetErr::TooLarge` if the blob exceeds `ASSET_MAX` (rejected before anything is written).
    pub fn set_background(&mut self, gif_bytes: &[u8]) -> Result<AssetId, AssetErr> {
        let asset = self.store.put_asset(gif_bytes)?;
        self.append(&WorldEvent::SetBackground { bg: Some(asset) });
        Ok(asset)
    }

    /// Clear back to the stage's own backdrop (another folded event, not a delete).
    pub fn clear_background(&mut self) {
        self.append(&WorldEvent::SetBackground { bg: None });
    }

    /// Background pixels for the current world (fold -> `AssetId` -> blob). `None` = stage default.
    pub fn background_bytes(&self) -> Option<Vec<u8>> {
        match self.background() {
            BgSource::Bytes(bytes) => Some(bytes),
            BgSource::None | BgSource::Missing(_) => None,
        }
    }

    /// The render source for the current background, distinguishing "no background set" from
    /// "one is set but the blob isn't held locally" (a rejoining peer: folded state names an
    /// `AssetId` before the P2P pull lands it). The shell renders `Bytes`, shows the stage default
    /// on `None`, and queues a Want on `Missing` (W2's job, not this fn's).
    pub fn background(&self) -> BgSource {
        match self.world().bg {
            None => BgSource::None,
            Some(id) => match self.store.get_asset(id) {
                Some(bytes) => BgSource::Bytes(bytes),
                None => BgSource::Missing(id),
            },
        }
    }

    // --- save slots (bookmarks) + reset. Restore reuses the frozen `Reset` event; nothing is deleted ---

    /// All saves for the current world, oldest first.
    pub fn slots(&self) -> Vec<Slot> {
        self.store.slots(self.current)
    }

    /// Rough stored footprint of the current world, for the size watch.
    pub fn cache_bytes(&self) -> u64 {
        self.store.cache_bytes(self.current)
    }

    /// Bookmark the current head under `label` (re-saving a label overwrites it). `at_ms` = caller's
    /// wall clock (the store has no clock). An empty log bookmarks genesis, so a restore still works.
    pub fn save_slot(&mut self, label: &str, at_ms: u64) -> Slot {
        let head = self.head.unwrap_or(GENESIS);
        let seq = Seq(self.store.since(self.current, None).len() as u64);
        let slot = Slot {
            label: label.to_string(),
            at_ms,
            head,
            seq,
        };
        self.store.put_slot(self.current, &slot);
        slot
    }

    /// Restore a saved point: append `Reset { to: head }` (fold jumps state back, history is kept).
    pub fn load_slot(&mut self, label: &str) {
        if let Some(s) = self.slots().into_iter().find(|s| s.label == label) {
            self.append(&WorldEvent::Reset { to: s.head });
        }
    }

    /// Forget a save (the log point it referenced stays in the log).
    pub fn delete_slot(&mut self, label: &str) {
        self.store.del_slot(self.current, label);
    }

    /// Reset home to the blank static defaults: `Reset { to: GENESIS }` clears the fold to genesis.
    /// Undo-able (it is a forward event) and it syncs like any other edit.
    pub fn reset_home(&mut self) {
        self.append(&WorldEvent::Reset { to: GENESIS });
    }

    /// Tick the auto-save clock by `dt` seconds; every `AUTOSAVE_SECS` it saves an `auto <ms>` slot,
    /// prunes the auto ring to `MAX_AUTOSAVES`, and returns the new cache size (so the caller can warn
    /// past the cap). Returns `None` on the frames that do not save.
    pub fn autosave(&mut self, dt: f32, now_ms: u64) -> Option<u64> {
        self.auto_accum += dt;
        if self.auto_accum < AUTOSAVE_SECS {
            return None;
        }
        self.auto_accum = 0.0;
        self.save_slot(&format!("{AUTO_PREFIX}{now_ms}"), now_ms);
        self.prune_autos();
        Some(self.cache_bytes())
    }

    /// Drop the oldest `auto ` saves beyond `MAX_AUTOSAVES`. Manual saves are untouched.
    fn prune_autos(&mut self) {
        let mut autos: Vec<Slot> = self
            .slots()
            .into_iter()
            .filter(|s| s.label.starts_with(AUTO_PREFIX))
            .collect();
        autos.sort_by_key(|s| s.at_ms);
        let excess = autos.len().saturating_sub(MAX_AUTOSAVES);
        for s in autos.into_iter().take(excess) {
            self.store.del_slot(self.current, &s.label);
        }
    }
}

/// A player's home world seed. Owner rides in `authored` so each home gets a distinct `WorldId`
/// (without an owner in the hash, every default home would collide on one id). The placeholder display
/// fields get overwritten by a live `PlayerJoin` carrying the real name/color once connected.
fn home_seed(owner: PlayerId) -> Seed {
    Seed {
        build: HOME_BUILD,
        rules: Tune::default(),
        stage: StageId::DEFAULT,
        bg: None,
        authored: vec![WorldEvent::PlayerJoin {
            player: owner,
            name: String::new(),
            color: 0,
            char_pick: 0,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::{BgSource, WorldRuntime};
    use crate::sim::world::store::{MemStore, WorldStore};
    use crate::sim::world::{PlayerId, WorldEvent, asset_id};

    fn runtime() -> WorldRuntime<MemStore> {
        WorldRuntime::boot(MemStore::new(), PlayerId([3u8; 32]))
    }

    // Task G2, item 3 (pick flow): picking a gif stages `SetBackground { bg: Some(asset_id(bytes)) }`.
    // Asserted on the raw staged event (not just the fold), matching the contract's wording exactly.
    #[test]
    fn set_background_stages_the_bytes_asset_event() {
        let mut rt = runtime();
        let bytes = b"a gif's worth of bytes".as_slice();

        let id = rt.set_background(bytes).expect("under ASSET_MAX");
        assert_eq!(id, asset_id(bytes)); // the returned id IS the content hash

        let log = rt.store.since(rt.world_id(), None);
        assert_eq!(
            log.last().map(|n| n.ev.clone()),
            Some(WorldEvent::SetBackground { bg: Some(id) })
        );
        assert_eq!(rt.world().bg, Some(id)); // and it folds to the same pointer
    }

    // Picking "none" stages `SetBackground { bg: None }`.
    #[test]
    fn clear_background_stages_a_none_event() {
        let mut rt = runtime();
        rt.set_background(b"something").unwrap();

        rt.clear_background();

        let log = rt.store.since(rt.world_id(), None);
        assert_eq!(
            log.last().map(|n| n.ev.clone()),
            Some(WorldEvent::SetBackground { bg: None })
        );
        assert_eq!(rt.world().bg, None);
    }

    // Task G2, item 5: bg never set -> `BgSource::None`.
    #[test]
    fn background_is_none_when_never_set() {
        let rt = runtime();
        assert_eq!(rt.background(), BgSource::None);
        assert_eq!(rt.background_bytes(), None);
    }

    // bg = Some(id) and the blob IS held -> `BgSource::Bytes` with the pixels.
    #[test]
    fn background_resolves_bytes_when_the_blob_is_held() {
        let mut rt = runtime();
        let bytes = b"held gif bytes".as_slice();
        rt.set_background(bytes).unwrap();

        assert_eq!(rt.background(), BgSource::Bytes(bytes.to_vec()));
        assert_eq!(rt.background_bytes(), Some(bytes.to_vec()));
    }

    // bg = Some(id) but the blob is missing locally (a rejoining peer before the P2P pull) ->
    // `BgSource::Missing(id)`, distinct from `None` — the shell still knows to queue a Want.
    #[test]
    fn background_reports_missing_when_the_blob_is_absent() {
        let mut rt = runtime();
        let bytes = b"never actually stored".as_slice();
        let id = asset_id(bytes);
        // append the pointer directly, bypassing `set_background` (which would `put_asset` first) —
        // this is exactly the rejoin shape: the fold names an id the local store never received.
        rt.append(&WorldEvent::SetBackground { bg: Some(id) });

        assert_eq!(rt.background(), BgSource::Missing(id));
        assert_eq!(rt.background_bytes(), None); // resolver still distinguishes it from "unset"
    }
}
