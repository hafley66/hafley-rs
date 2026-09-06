use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Copy,
            Clone,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Debug,
            Serialize,
            Deserialize,
        )]
        #[repr(transparent)]
        pub struct $name(pub u16);

        impl $name {
            pub const fn new(raw: u16) -> Self {
                Self(raw)
            }

            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_type!(EntityId);
id_type!(KindId);
id_type!(GeometryId);
id_type!(MaterialId);
id_type!(PaintId);
id_type!(ProgramId);
id_type!(PrefabId);
id_type!(StateId);
id_type!(EventId);
id_type!(SelectorId);
id_type!(SymbolId);
id_type!(RuleSetId);
id_type!(RelationKindId);

impl EntityId {
    /// Missing entity/reference sentinel. Real arenas must never allocate this id.
    pub const NONE: Self = Self(u16::MAX);

    pub const fn is_none(self) -> bool {
        self.0 == u16::MAX
    }
}
