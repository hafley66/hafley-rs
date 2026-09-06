use super::{EntityId, RelationKindId};
use serde::{Deserialize, Serialize};

/// One typed directed edge in the rollback graph.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Relation {
    pub kind: RelationKindId,
    pub from: EntityId,
    pub to: EntityId,
    pub since: u32,
}

impl Relation {
    const EMPTY: Self = Self {
        kind: RelationKindId(0),
        from: EntityId::NONE,
        to: EntityId::NONE,
        since: 0,
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RelationWriteOutcome {
    Connected,
    AlreadyConnected,
    Blocked { existing: EntityId },
    Replaced { displaced: EntityId },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RelationError {
    Full,
}

/// Fixed-capacity relation storage. Iteration order is insertion order and replacement keeps its
/// slot, making command replay and debug traces deterministic.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RelationTable<const N: usize> {
    slots: [Relation; N],
    len: u16,
}

impl<const N: usize> RelationTable<N> {
    pub const EMPTY: Self = Self {
        slots: [Relation::EMPTY; N],
        len: 0,
    };

    pub fn as_slice(&self) -> &[Relation] {
        &self.slots[..self.len as usize]
    }

    pub fn find(&self, kind: RelationKindId, from: EntityId, to: EntityId) -> Option<Relation> {
        self.as_slice()
            .iter()
            .copied()
            .find(|edge| edge.kind == kind && edge.from == from && edge.to == to)
    }

    pub fn incoming(&self, kind: RelationKindId, to: EntityId) -> Option<Relation> {
        self.as_slice()
            .iter()
            .copied()
            .find(|edge| edge.kind == kind && edge.to == to)
    }

    /// Set insertion: keep every distinct edge, including many incoming edges of one kind.
    pub fn add(&mut self, relation: Relation) -> Result<RelationWriteOutcome, RelationError> {
        if self
            .find(relation.kind, relation.from, relation.to)
            .is_some()
        {
            return Ok(RelationWriteOutcome::AlreadyConnected);
        }
        self.push(relation)
    }

    /// Conditional slot write keyed by `(kind, to)`.
    pub fn set_if_empty(
        &mut self,
        relation: Relation,
    ) -> Result<RelationWriteOutcome, RelationError> {
        if self
            .find(relation.kind, relation.from, relation.to)
            .is_some()
        {
            return Ok(RelationWriteOutcome::AlreadyConnected);
        }
        if let Some(existing) = self.incoming(relation.kind, relation.to) {
            return Ok(RelationWriteOutcome::Blocked {
                existing: existing.from,
            });
        }
        self.push(relation)
    }

    /// Unconditional slot write keyed by `(kind, to)`.
    pub fn set(&mut self, relation: Relation) -> Result<RelationWriteOutcome, RelationError> {
        if self
            .find(relation.kind, relation.from, relation.to)
            .is_some()
        {
            return Ok(RelationWriteOutcome::AlreadyConnected);
        }
        if let Some(index) = self
            .as_slice()
            .iter()
            .position(|edge| edge.kind == relation.kind && edge.to == relation.to)
        {
            let displaced = self.slots[index].from;
            self.slots[index] = relation;
            return Ok(RelationWriteOutcome::Replaced { displaced });
        }
        self.push(relation)
    }

    fn push(&mut self, relation: Relation) -> Result<RelationWriteOutcome, RelationError> {
        let index = self.len as usize;
        if index >= N || self.len == u16::MAX {
            return Err(RelationError::Full);
        }
        self.slots[index] = relation;
        self.len += 1;
        Ok(RelationWriteOutcome::Connected)
    }

    pub fn remove(&mut self, kind: RelationKindId, from: EntityId, to: EntityId) -> bool {
        let Some(index) = self
            .as_slice()
            .iter()
            .position(|edge| edge.kind == kind && edge.from == from && edge.to == to)
        else {
            return false;
        };
        let last = self.len as usize - 1;
        for slot in index..last {
            self.slots[slot] = self.slots[slot + 1];
        }
        self.slots[last] = Relation::EMPTY;
        self.len -= 1;
        true
    }
}

impl<const N: usize> Default for RelationTable<N> {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: RelationKindId = RelationKindId(3);
    const NODE: EntityId = EntityId(9);

    fn edge(from: u16, since: u32) -> Relation {
        Relation {
            kind: LINK,
            from: EntityId(from),
            to: NODE,
            since,
        }
    }

    #[test]
    fn slot_writes_are_generic_and_deterministic() {
        let mut table = RelationTable::<2>::EMPTY;
        assert_eq!(
            table.set_if_empty(edge(1, 10)),
            Ok(RelationWriteOutcome::Connected)
        );
        assert_eq!(
            table.set_if_empty(edge(2, 11)),
            Ok(RelationWriteOutcome::Blocked {
                existing: EntityId(1)
            })
        );
        assert_eq!(
            table.set(edge(2, 12)),
            Ok(RelationWriteOutcome::Replaced {
                displaced: EntityId(1)
            })
        );
        assert_eq!(table.as_slice(), &[edge(2, 12)]);
    }

    #[test]
    fn add_keeps_general_graph_edges_many_to_one() {
        let mut table = RelationTable::<2>::EMPTY;
        table.add(edge(1, 10)).unwrap();
        table.add(edge(2, 11)).unwrap();
        assert_eq!(table.as_slice(), &[edge(1, 10), edge(2, 11)]);
    }
}
