//! The content-keyed extraction cache. A hit must skip the parse, which an
//! equality assertion cannot prove; the discriminating receipt is the counter.
//!
//! CONTROL: `cache::EXTRACTIONS` counts computes inside `get_or_extract`; an
//! identical-bytes second call leaves it unmoved.
//! SABOTAGE 1, drop the cache lookup and always extract: every call bumps
//! `EXTRACTIONS`, so `hit_skips_the_parse` goes RED.
//! SABOTAGE 2, drop the mask from the key (ContentId + lang only): the narrower
//! mask collides with `FamilyMask::ALL`, so `different_mask` goes RED.
//! SABOTAGE 3, make the weigher return a constant (1) instead of the byte
//! estimate: `eviction_binds_at_the_weight_cap` never evicts and goes RED.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use sprefa_extract::{
    cache::{self, CacheKey},
    content_id_of, dispatch, source_for, FamilyMask, RyiOutput,
};

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static CACHE_LOCK: Mutex<()> = Mutex::new(());
    CACHE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const PATH_A: &str = "blob_cache_a.rs";
const BLOB: &[u8] = b"pub fn trim(value: String) -> String {\n    value\n}\n";

// Each counter test extracts a blob unique to it: the global cache is shared
// across the whole test binary, so a reused blob would already be warm.
const HIT_BLOB: &[u8] = b"pub fn hit_unique() {}\n";
const TWO_BLOB: &[u8] = b"function twice() { return () => 2; }\n";
const TWO_PATH_A: &str = "blob_cache_a.ts";
const TWO_PATH_B: &str = "blob_cache_b.ts";
const MASK_BLOB: &[u8] = b"pub fn mask_unique() {}\n";

#[test]
fn hit_skips_the_parse() {
    let _guard = lock();
    let start = cache::EXTRACTIONS.load(Ordering::Relaxed);
    let first = dispatch(PATH_A, HIT_BLOB, FamilyMask::ALL).expect("rust source");
    assert_eq!(cache::EXTRACTIONS.load(Ordering::Relaxed), start + 1);
    let second = dispatch(PATH_A, HIT_BLOB, FamilyMask::ALL).expect("rust source");
    assert_eq!(
        cache::EXTRACTIONS.load(Ordering::Relaxed),
        start + 1,
        "identical bytes must not re-parse"
    );
    assert!(
        Arc::ptr_eq(&first, &second),
        "hit returns the same shared entry"
    );
}

#[test]
fn two_paths_one_blob_have_distinct_outputs() {
    let _guard = lock();
    let start = cache::EXTRACTIONS.load(Ordering::Relaxed);
    let first = dispatch(TWO_PATH_A, TWO_BLOB, FamilyMask::ALL).expect("TypeScript source");
    let second = dispatch(TWO_PATH_B, TWO_BLOB, FamilyMask::ALL).expect("TypeScript source");
    assert_eq!(
        cache::EXTRACTIONS.load(Ordering::Relaxed),
        start + 2,
        "path-dependent output requires a separate extraction"
    );
    assert_ne!(wire_bytes(&first), wire_bytes(&second));
}

#[test]
fn different_mask_is_a_different_entry() {
    let _guard = lock();
    let start = cache::EXTRACTIONS.load(Ordering::Relaxed);
    let narrow = FamilyMask {
        cst: true,
        ..FamilyMask::NONE
    };
    dispatch(PATH_A, MASK_BLOB, narrow).expect("rust source");
    dispatch(PATH_A, MASK_BLOB, FamilyMask::ALL).expect("rust source");
    assert_eq!(
        cache::EXTRACTIONS.load(Ordering::Relaxed),
        start + 2,
        "a different mask is a different entry and must re-parse"
    );
}

#[test]
fn eviction_binds_at_the_weight_cap() {
    let contents: Vec<(String, Vec<u8>)> = (0..6)
        .map(|index| {
            (
                format!("evict_{index}.rs"),
                format!(
                    "pub fn f{index}() {{ let s = \"{}\"; }}\n",
                    "x".repeat(4000)
                )
                .into_bytes(),
            )
        })
        .collect();

    let source = source_for("evict_0.rs").expect("rust source");
    let outputs: Vec<Arc<RyiOutput>> = contents
        .iter()
        .map(|(path, body)| Arc::new(source.extract(path, body, FamilyMask::ALL)))
        .collect();
    let one_weight = cache::estimate_bytes(&outputs[0]) as u64;
    let keys: Vec<CacheKey> = contents
        .iter()
        .map(|(path, body)| CacheKey::new(content_id_of(body), path, "rust", FamilyMask::ALL))
        .collect();

    let tight = cache::cache_with_capacity(one_weight * 2);
    for (key, output) in keys.iter().zip(outputs.iter()) {
        tight.insert(key.clone(), output.clone());
    }
    assert!(tight.weight() <= one_weight * 2, "weight bound must bind");
    let present: usize = keys.iter().filter(|key| tight.peek(*key).is_some()).count();
    assert!(
        present <= 2,
        "a cap of two entries holds at most two, but {present} survive"
    );
    let early_present: usize = keys[..4]
        .iter()
        .filter(|key| tight.peek(*key).is_some())
        .count();
    assert!(
        early_present < 4,
        "an earlier entry must be evicted past the cap"
    );
    assert!(tight.peek(&keys[5]).is_some(), "a later entry survives");
}

#[test]
fn cached_and_uncached_wire_output_are_identical() {
    let _guard = lock();
    let source = source_for(PATH_A).expect("rust source");
    let uncached = source.extract(PATH_A, BLOB, FamilyMask::ALL);
    let uncached_wire = wire_bytes(&uncached);

    let cached = dispatch(PATH_A, BLOB, FamilyMask::ALL).expect("rust source");
    let cached_wire = wire_bytes(&cached);
    assert_eq!(
        cached_wire, uncached_wire,
        "cached and uncached wire output match"
    );
}

fn wire_bytes(output: &RyiOutput) -> Vec<u8> {
    sprefa_extract::flatten(output)
        .iter()
        .flat_map(|fact| serde_json::to_vec(fact).expect("fact serializes"))
        .collect()
}

/// The weight must cover what an output holds. TSI rows were ~80% of a
/// TypeScript output's heap and sat outside the estimate, and their vector kept
/// the in-place collect's `FlatFact` allocation (6.5x the rows); a 512 MiB cap
/// held ~1.5 GiB in the daemon.
#[test]
fn weight_covers_tsi_rows_and_they_hold_no_spare_capacity() {
    // `dispatch_uncached` bumps `EXTRACTIONS`, which the counter tests read.
    let _guard = lock();
    let source: String = (0..200)
        .map(|index| {
            format!("export function f{index}(a: Map<string, number[]>, b: Set<T{index}>): Promise<U{index}> {{ return g(a, b); }}\n")
        })
        .collect();
    let output = sprefa_extract::dispatch::dispatch_uncached(
        "weight_tsi.ts",
        source.as_bytes(),
        FamilyMask { types: true, call: true, ..FamilyMask::NONE },
    )
    .expect("TypeScript source");
    let tsi = &output.types.as_ref().expect("type plane").aux.tsi;
    let row_floor = tsi.len() * size_of::<sprefa_extract::tsi::FactOut>();
    assert_eq!(
        (
            tsi.len() > 1000,
            tsi.capacity() == tsi.len(),
            cache::estimate_bytes(&output) > row_floor + source.len(),
        ),
        (true, true, true),
        "rows {} capacity {} estimate {} floor {}",
        tsi.len(),
        tsi.capacity(),
        cache::estimate_bytes(&output),
        row_floor + source.len()
    );
}
