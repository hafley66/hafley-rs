//! Content-addressed asset identity + P2P chunk transfer, pure and game-agnostic. Moved out of
//! `v1::{world,net::assets}` so the V4 path never imports through the legacy module; v1's world
//! store and lobby keep using these items from here.
//!
//! Model: a joiner sends [`AssetMsg::AssetWant`] for each missing blob and the holder answers with
//! a run of [`AssetMsg::AssetChunk`]s produced by [`chunk`]. A [`Reassembler`] on the joiner's side
//! buffers chunks by `idx` until `total` are held, then self-verifies via `blake3` (the same
//! [`asset_id`] every store uses) before releasing bytes — a tampered or truncated transfer never
//! surfaces wrong bytes.

use serde::{Deserialize, Serialize};

/// A content-addressed asset (gif background, authored stage blob, the game document): fetched by
/// hash, never a path/URL, so it is self-verifying on fetch.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct AssetId(pub [u8; 32]);

/// An asset's identity = `blake3(bytes)`. The id IS the content, so a put is idempotent and a fetch
/// self-verifies. Shared by every `WorldStore` backend so their asset ids match.
pub fn asset_id(bytes: &[u8]) -> AssetId {
    AssetId(*blake3::hash(bytes).as_bytes())
}

/// Largest blob a `WorldStore::put_asset` accepts (plans/world-protocol.md §T, task W1): 8 MiB. Named
/// so both backends and the peer-transfer cap (§T task W2's `ASSET_CHUNK` budget) read the same value.
pub const ASSET_MAX: usize = 8 * 1024 * 1024;

/// WebRTC data-channel-safe chunk size (plans/world-protocol.md §T). `ASSET_MAX` is the storage cap;
/// this is purely the transport-side split size, unrelated to that cap.
pub const ASSET_CHUNK: usize = 16 * 1024;

/// Asset sync messages carried over the same peer channel as the rest of the wire. Append-only +
/// `#[non_exhaustive]`, bincode-framed, like `WorldEvent` (plans/world-protocol.md §2): new variants
/// go LAST, never reordered/removed, so old bytes keep decoding.
// reuse-kit-library(asset-wire): shared content-addressed asset identity + P2P chunk transfer (AssetMsg/Reassembler/chunk/asset_id) consumed by both V1 (v1/net/assets host_answer) and V4 (v4_net::AssetCatalog).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AssetMsg {
    /// "I'm missing this blob" — the joiner's half of a `missing_assets` diff.
    AssetWant(AssetId),
    /// One chunk of a blob transfer. `idx`/`total` make delivery order and duplicates irrelevant to
    /// the receiver; `bytes.len() <= ASSET_CHUNK` for every chunk `host_answer`/`chunk` produce.
    AssetChunk {
        id: AssetId,
        idx: u32,
        total: u32,
        bytes: Vec<u8>,
    },
    // APPEND NEW VARIANTS HERE, at the end. Never reorder, never remove.
}

/// Number of `ASSET_CHUNK`-sized pieces `len` bytes split into. `ceil(len / ASSET_CHUNK)`, floored to
/// 1 so a zero-length blob still gets one (empty) chunk — otherwise a zero-byte asset would never
/// tell a `Reassembler` how many pieces (`total`) to wait for.
pub(crate) fn chunk_count(len: usize) -> u32 {
    (len.div_ceil(ASSET_CHUNK)).max(1) as u32
}

/// Split `bytes` into the `AssetChunk` messages that reassemble to it, in order, each no larger than
/// `ASSET_CHUNK`. `id` is the asset identity the receiver verifies against once reassembled — the
/// caller supplies it (normally `asset_id(bytes)`) rather than this fn recomputing it every call.
pub fn chunk(id: AssetId, bytes: &[u8]) -> Vec<AssetMsg> {
    let total = chunk_count(bytes.len());
    (0..total)
        .map(|idx| {
            let start = idx as usize * ASSET_CHUNK;
            let end = (start + ASSET_CHUNK).min(bytes.len());
            AssetMsg::AssetChunk {
                id,
                idx,
                total,
                bytes: bytes[start..end].to_vec(),
            }
        })
        .collect()
}

/// Idx-addressed reassembly buffer for one peer's inbound chunks. Bounded: a chunk for an id never
/// [`want`](Reassembler::want)ed is dropped before it ever allocates a buffer, so an unsolicited flood
/// can't grow memory. Out-of-order delivery and duplicates are both fine (last chunk at a given `idx`
/// wins); bytes release only once every `idx` in `0..total` is held, and only if `blake3(bytes)`
/// (via `asset_id`, the same fn `WorldStore` uses) matches the wanted id — a mismatch drops the
/// completed buffer and reports nothing, so a corrupt transfer never surfaces wrong bytes.
#[derive(Default)]
pub struct Reassembler {
    wanted: std::collections::HashSet<AssetId>,
    /// pub(crate): v1's transfer tests assert no buffer ever allocates for an unwanted id.
    pub(crate) partial: std::collections::HashMap<AssetId, Vec<Option<Vec<u8>>>>,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `id` as wanted. Call before (or as) the `AssetWant` for it goes out — chunks for an id
    /// that was never marked wanted are ignored by [`ingest`](Reassembler::ingest).
    pub fn want(&mut self, id: AssetId) {
        self.wanted.insert(id);
    }

    /// Feed one inbound message. Non-chunk messages and chunks for an unwanted id are ignored
    /// (`None`). Once every `idx` for `id` has landed, verifies the reassembled bytes against `id`'s
    /// hash: `Some(bytes)` on a match, `None` (and the partial buffer dropped) on a mismatch.
    pub fn ingest(&mut self, msg: &AssetMsg) -> Option<Vec<u8>> {
        let AssetMsg::AssetChunk {
            id,
            idx,
            total,
            bytes,
        } = msg
        else {
            return None;
        };
        if !self.wanted.contains(id) {
            return None; // never asked for this id -- ignore, no buffer growth
        }
        // Envelope sanity: a legit sender never exceeds these (chunk/host_answer can't), so an
        // over-claimed `total` or oversized payload is a lying peer -- drop it before it sizes a
        // buffer. Bounds the whole partial map at wanted-count * ASSET_MAX.
        if *total == 0 || *total > chunk_count(ASSET_MAX) || bytes.len() > ASSET_CHUNK {
            return None;
        }
        let total = *total as usize;
        let slots = self.partial.entry(*id).or_insert_with(|| vec![None; total]);
        if (*idx as usize) < slots.len() {
            slots[*idx as usize] = Some(bytes.clone());
        }
        if slots.iter().any(Option::is_none) {
            return None; // still incomplete
        }
        let (_, slots) = self.partial.remove_entry(id).expect("just inserted");
        let whole: Vec<u8> = slots.into_iter().flatten().flatten().collect();
        if asset_id(&whole) == *id {
            Some(whole)
        } else {
            None // tampered/truncated -- rejected, nothing stored
        }
    }
}
