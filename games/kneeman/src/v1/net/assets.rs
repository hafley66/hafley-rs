//! v1 half of the P2P asset transfer (plans/world-protocol.md §T, task W2): the host-side answer
//! built from a `WorldStore`, plus the byte-freeze golden tests. The pure wire protocol
//! (AssetId/AssetMsg/chunk/Reassembler) moved to the shared shell module `crate::asset_wire` so
//! the V4 path never imports through `v1`; everything is re-exported here for v1 callers.

pub use crate::asset_wire::{
    ASSET_CHUNK, ASSET_MAX, AssetId, AssetMsg, Reassembler, asset_id, chunk,
};
use crate::v1::world::store::WorldStore;

/// Pure host-side answer to one `AssetWant`: the chunk messages to send back, built from whatever the
/// host's own `WorldStore` holds. Empty if the store doesn't have the blob, OR (belt + braces with
/// task W1.4, which should already refuse to store an over-cap blob) if it somehow holds one over
/// `ASSET_MAX` — never chunk-and-send an oversized blob just because a row slipped past that guard.
pub fn host_answer<S: WorldStore>(store: &S, want: AssetId) -> Vec<AssetMsg> {
    match store.get_asset(want) {
        Some(bytes) if bytes.len() <= ASSET_MAX => chunk(want, &bytes),
        _ => Vec::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Byte-freeze: the exact on-wire encoding of every `AssetMsg` variant (plans/world-protocol.md §2's
// discipline test, same pattern as core/src/world/mod.rs `mod golden`). To add a variant at the END:
// keep all lines below, append the new variant + its golden line.
// ─────────────────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod golden {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    fn frozen_msgs() -> Vec<AssetMsg> {
        vec![
            AssetMsg::AssetWant(AssetId([7u8; 32])),
            AssetMsg::AssetChunk {
                id: AssetId([8u8; 32]),
                idx: 1,
                total: 3,
                bytes: vec![9, 10, 11],
            },
        ]
    }

    #[test]
    fn assetmsg_bytes_are_frozen() {
        let expect = [
            "000000000707070707070707070707070707070707070707070707070707070707070707",
            "01000000080808080808080808080808080808080808080808080808080808080808080801000000030000000300000000000000090a0b",
        ];
        for (m, want) in frozen_msgs().iter().zip(expect) {
            assert_eq!(hex(&bincode::serialize(m).unwrap()), want); // reordering/moving a field breaks this exact byte string
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::world::store::MemStore;
    use crate::v1::world::{BuildVersion, Seed, StageId};

    // ── item 2: chunker split+reassemble is identity, chunk count == ceil(len/ASSET_CHUNK) ──

    fn probe_lengths() -> Vec<usize> {
        vec![
            0,
            1,
            ASSET_CHUNK,
            ASSET_CHUNK + 1,
            3 * ASSET_CHUNK + ASSET_CHUNK / 2,
        ]
    }

    fn expected_count(len: usize) -> u32 {
        if len == 0 {
            1
        } else {
            len.div_ceil(ASSET_CHUNK) as u32
        }
    }

    fn sample_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn chunk_reassemble_is_identity_for_every_probe_length() {
        for len in probe_lengths() {
            let bytes = sample_bytes(len);
            let id = asset_id(&bytes);
            let chunks = chunk(id, &bytes);
            assert_eq!(chunks.len() as u32, expected_count(len));

            let mut r = Reassembler::new();
            r.want(id);
            let mut got = None;
            for c in &chunks {
                if let Some(bytes) = r.ingest(c) {
                    got = Some(bytes);
                }
            }
            assert_eq!(got, Some(bytes));
        }
    }

    // ── item 3: reassembly is idx-addressed -- out-of-order + duplicate delivery still yields the
    // exact bytes; a missing chunk leaves it incomplete (no bytes released) ──

    #[test]
    fn reassembly_tolerates_out_of_order_and_duplicate_chunks() {
        let bytes = sample_bytes(3 * ASSET_CHUNK + 7);
        let id = asset_id(&bytes);
        let mut chunks = chunk(id, &bytes);
        assert!(chunks.len() > 2);
        let last = chunks.len() - 1;
        chunks.swap(0, last); // shuffle delivery order
        chunks.push(chunks[0].clone()); // + a duplicate

        let mut r = Reassembler::new();
        r.want(id);
        let mut got = None;
        for c in &chunks {
            if let Some(b) = r.ingest(c) {
                got = Some(b);
            }
        }
        assert_eq!(got, Some(bytes));
    }

    #[test]
    fn missing_chunk_leaves_reassembly_incomplete() {
        let bytes = sample_bytes(3 * ASSET_CHUNK + 7);
        let id = asset_id(&bytes);
        let mut chunks = chunk(id, &bytes);
        chunks.remove(1); // drop one piece from the middle

        let mut r = Reassembler::new();
        r.want(id);
        for c in &chunks {
            assert_eq!(r.ingest(c), None); // never completes -- no bytes released
        }
    }

    // ── item 4: receiver rejects a completed blob whose blake3 != the wanted id -- nothing stored ──

    #[test]
    fn tampered_completed_blob_is_rejected() {
        let bytes = sample_bytes(ASSET_CHUNK + 5);
        let id = asset_id(&bytes);
        let mut chunks = chunk(id, &bytes);
        // corrupt one byte inside a chunk after chunking -- id no longer matches the payload.
        if let AssetMsg::AssetChunk { bytes, .. } = &mut chunks[0] {
            bytes[0] ^= 0xff;
        }

        let mut r = Reassembler::new();
        r.want(id);
        let mut got = None;
        for c in &chunks {
            if let Some(b) = r.ingest(c) {
                got = Some(b);
            }
        }
        assert_eq!(got, None); // hash mismatch on completion -- rejected, nothing surfaced
    }

    // ── item 5: receiver ignores chunks for an id it never wanted (no unbounded buffer growth) ──

    #[test]
    fn unwanted_id_chunks_are_ignored() {
        let bytes = sample_bytes(10);
        let id = asset_id(&bytes);
        let chunks = chunk(id, &bytes);

        let mut r = Reassembler::new(); // note: never called r.want(id)
        for c in &chunks {
            assert_eq!(r.ingest(c), None);
        }
        assert!(r.partial.is_empty()); // never allocated a buffer for the unwanted id
    }

    // a lying envelope on a WANTED id (over-claimed total / oversized payload) is dropped before it
    // sizes a buffer -- a hostile answer to a legit Want can't force a giant allocation.
    #[test]
    fn lying_envelope_on_a_wanted_id_is_dropped() {
        let id = asset_id(b"whatever");
        let mut r = Reassembler::new();
        r.want(id);

        let over_total = AssetMsg::AssetChunk {
            id,
            idx: 0,
            total: u32::MAX,
            bytes: vec![1, 2, 3],
        };
        assert_eq!(r.ingest(&over_total), None);
        assert!(r.partial.is_empty());

        let over_payload = AssetMsg::AssetChunk {
            id,
            idx: 0,
            total: 1,
            bytes: vec![0u8; ASSET_CHUNK + 1],
        };
        assert_eq!(r.ingest(&over_payload), None);
        assert!(r.partial.is_empty());
    }

    // ── item 7: a Want for a blob the host holds at > ASSET_MAX answers with nothing; every chunk a
    // normal answer DOES produce stays within the ASSET_CHUNK + envelope budget ──

    /// A `WorldStore` double whose `get_asset` hands back a blob bigger than `ASSET_MAX` -- something
    /// the real backends' `put_asset` (task W1.4) should never let happen, but `host_answer` guards
    /// against it anyway (belt + braces), so the test needs a store that can misbehave on purpose.
    struct OversizeStore(Vec<u8>);

    impl WorldStore for OversizeStore {
        fn publish(&mut self, seed: &Seed) -> crate::v1::world::WorldId {
            crate::v1::world::world_id(seed)
        }
        fn seed(&self, _id: crate::v1::world::WorldId) -> Option<Seed> {
            None
        }
        fn append(
            &mut self,
            _id: crate::v1::world::WorldId,
            _ev: &crate::v1::world::WorldEvent,
        ) -> crate::v1::world::EventId {
            crate::v1::world::EventId([0u8; 32])
        }
        fn head(&self, _id: crate::v1::world::WorldId) -> Option<crate::v1::world::EventId> {
            None
        }
        fn since(
            &self,
            _id: crate::v1::world::WorldId,
            _from: Option<crate::v1::world::EventId>,
        ) -> Vec<crate::v1::world::Node> {
            Vec::new()
        }
        fn has(&self, _id: crate::v1::world::WorldId, _ev: crate::v1::world::EventId) -> bool {
            false
        }
        fn put_snapshot(
            &mut self,
            _id: crate::v1::world::WorldId,
            _upto: crate::v1::world::EventId,
            _blob: &[u8],
        ) {
        }
        fn get_snapshot(
            &self,
            _id: crate::v1::world::WorldId,
        ) -> Option<(crate::v1::world::EventId, Vec<u8>)> {
            None
        }
        fn compact(&mut self, _id: crate::v1::world::WorldId, _upto: crate::v1::world::EventId) {}
        fn put_asset(&mut self, _bytes: &[u8]) -> Result<AssetId, crate::v1::world::AssetErr> {
            unimplemented!("test double: assets are seeded directly, never put")
        }
        fn get_asset(&self, _id: AssetId) -> Option<Vec<u8>> {
            Some(self.0.clone()) // hands back the oversized blob regardless of the id asked for
        }
        fn put_slot(
            &mut self,
            _id: crate::v1::world::WorldId,
            _slot: &crate::v1::world::store::Slot,
        ) {
        }
        fn slots(&self, _id: crate::v1::world::WorldId) -> Vec<crate::v1::world::store::Slot> {
            Vec::new()
        }
        fn del_slot(&mut self, _id: crate::v1::world::WorldId, _label: &str) {}
        fn cache_bytes(&self, _id: crate::v1::world::WorldId) -> u64 {
            0
        }
    }

    #[test]
    fn oversize_blob_is_answered_with_nothing_and_normal_chunks_stay_in_budget() {
        let oversized = OversizeStore(vec![0u8; ASSET_MAX + 1]);
        let reply = host_answer(&oversized, AssetId([1u8; 32]));
        assert!(reply.is_empty()); // over ASSET_MAX -- answered with nothing, belt + braces with W1.4

        // a normal in-budget answer: every produced chunk's payload fits ASSET_CHUNK, and the whole
        // wire message (payload + the small idx/total/id envelope) stays close to it too.
        let mut normal = MemStore::new();
        let bytes = sample_bytes(3 * ASSET_CHUNK + 7);
        let id = normal.put_asset(&bytes).unwrap();
        let chunks = host_answer(&normal, id);
        assert!(!chunks.is_empty());
        for c in &chunks {
            let AssetMsg::AssetChunk { bytes, .. } = c else {
                panic!("host_answer only ever produces AssetChunk")
            };
            assert!(bytes.len() <= ASSET_CHUNK);
            let wire = bincode::serialize(c).unwrap();
            assert!(wire.len() <= ASSET_CHUNK + 64); // payload + a small fixed envelope, never more
        }
    }

    // ── item 6: the join flow end to end -- two MemStores, an in-memory Vec-of-messages pipe, no
    // sockets. Host has seed{bg: Some(a)} + blob a; joiner publishes the same seed, misses a, drives a
    // Want, host answers with chunks, joiner ends up with the exact bytes. Re-join sends zero messages
    // (nothing missing). ──

    fn seed_with_bg(bg: Option<AssetId>) -> Seed {
        Seed {
            build: BuildVersion(1),
            rules: crate::v1::Tune::default(),
            stage: StageId::DEFAULT,
            bg,
            authored: vec![],
        }
    }

    /// Drive one `missing_assets` -> `Want`s -> `host_answer` -> `Reassembler` -> `put_asset` round
    /// trip over the in-memory pipe. Returns the number of `AssetWant`s the joiner sent (so a re-join
    /// with nothing missing can assert zero messages).
    fn sync_missing_assets(
        joiner: &mut MemStore,
        host: &MemStore,
        world: crate::v1::world::WorldId,
    ) -> usize {
        let missing = joiner.missing_assets(world);
        let wants: Vec<AssetMsg> = missing.iter().map(|a| AssetMsg::AssetWant(*a)).collect();

        let mut r = Reassembler::new();
        for a in &missing {
            r.want(*a);
        }

        // the "pipe": every Want answered in turn, replies ingested as they arrive.
        for w in &wants {
            let AssetMsg::AssetWant(id) = w else {
                continue;
            };
            for chunk_msg in host_answer(host, *id) {
                if let Some(bytes) = r.ingest(&chunk_msg) {
                    joiner.put_asset(&bytes).unwrap();
                }
            }
        }
        wants.len()
    }

    #[test]
    fn join_flow_transfers_the_missing_asset_then_rejoin_is_silent() {
        let bytes_a = sample_bytes(3 * ASSET_CHUNK + 123);

        let mut host = MemStore::new();
        let a = host.put_asset(&bytes_a).unwrap();
        let world = host.publish(&seed_with_bg(Some(a)));

        let mut joiner = MemStore::new();
        let world2 = joiner.publish(&seed_with_bg(Some(a)));
        assert_eq!(world, world2); // same seed content -> same WorldId, no coordination needed

        assert_eq!(joiner.missing_assets(world), vec![a]);
        let sent = sync_missing_assets(&mut joiner, &host, world);
        assert_eq!(sent, 1);

        assert_eq!(joiner.get_asset(a), host.get_asset(a));
        assert_eq!(joiner.get_asset(a).as_deref(), Some(bytes_a.as_slice()));
        assert!(joiner.missing_assets(world).is_empty());

        // re-join: nothing missing -> zero Want messages, zero chunks.
        let sent_again = sync_missing_assets(&mut joiner, &host, world);
        assert_eq!(sent_again, 0);
    }
}
