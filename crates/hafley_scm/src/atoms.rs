use std::fmt;
use serde::Serialize;

/// Dense u32 into the per-file `Strings` interner.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NameId(pub u32);

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NameId({})", self.0)
    }
}

/// The per-file string interner backing every `NameId`. One per extraction; the
/// dispatch creates it, passes `&mut` to each projector, keeps it so the wire
/// flatten can resolve `NameId -> &str`. Dedups on insert.
#[derive(Default)]
pub struct Strings {
    map: std::collections::HashMap<String, NameId>,
    names: Vec<String>,
}

impl Strings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `s`, returning a stable `NameId`. Byte-identical strings share one id.
    pub fn intern(&mut self, s: &str) -> NameId {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = NameId(self.names.len() as u32);
        self.map.insert(s.to_string(), id);
        self.names.push(s.to_string());
        id
    }

    pub fn lookup(&self, id: NameId) -> &str {
        &self.names[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl Strings {
    /// Approximate heap footprint of the interned strings, for the cache weigher.
    /// Moves with the real size; not exact.
    pub fn heap_bytes(&self) -> usize {
        self.map.len() * size_of::<(String, NameId)>()
            + self
                .names
                .iter()
                .map(|name| name.capacity() + size_of::<String>())
                .sum::<usize>()
    }
}

/// Local index into one file's node vec; flattened to a span at the wire.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeRef(pub u32);

/// The flat family discriminant at the seam only (the wire, the ratchet key).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FamilyTag {
    Df,
    Flow,
    Call,
    Type,
    Module,
    Cst,
    Cfg,
    Data,
}

/// A digest of the file set that affects resolution (which files exist + their
/// manifest membership), folded from the corpus so two identical blobs in
/// identical file-set contexts share phase-2 work. The middle component of the
/// phase-2 cache key (see `Resolve`). Spec: seed `_1_mask.rs`:78-82. Declared
/// here; NOT computed yet (lands with the phase-2 cache).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProjectDigest(pub [u8; 16]);
