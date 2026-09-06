use super::RuleSetId;
use serde::{Deserialize, Serialize};

/// Runtime-selected rule sets from the loaded document. Bit storage keeps selection bounded,
/// rollback-copyable, and independent of authored names such as `melee` or `ultimate`.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ActiveSets(u64);

impl ActiveSets {
    pub const EMPTY: Self = Self(0);
    pub const CAPACITY: usize = u64::BITS as usize;

    pub fn contains(self, set: RuleSetId) -> bool {
        mask(set).is_some_and(|mask| self.0 & mask != 0)
    }

    pub fn add(&mut self, set: RuleSetId) -> Result<bool, SetError> {
        let mask = mask(set).ok_or(SetError::OutOfRange(set))?;
        let changed = self.0 & mask == 0;
        self.0 |= mask;
        Ok(changed)
    }

    pub fn remove(&mut self, set: RuleSetId) -> Result<bool, SetError> {
        let mask = mask(set).ok_or(SetError::OutOfRange(set))?;
        let changed = self.0 & mask != 0;
        self.0 &= !mask;
        Ok(changed)
    }

    pub fn iter(self) -> impl Iterator<Item = RuleSetId> {
        (0..Self::CAPACITY)
            .filter(move |index| self.0 & (1_u64 << index) != 0)
            .map(|index| RuleSetId(index as u16))
    }
}

fn mask(set: RuleSetId) -> Option<u64> {
    (set.index() < ActiveSets::CAPACITY).then(|| 1_u64 << set.index())
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SetError {
    OutOfRange(RuleSetId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_sets_are_runtime_collection_state() {
        let mut sets = ActiveSets::EMPTY;
        assert_eq!(sets.add(RuleSetId(3)), Ok(true));
        assert_eq!(sets.add(RuleSetId(3)), Ok(false));
        assert!(sets.contains(RuleSetId(3)));
        assert_eq!(sets.iter().collect::<Vec<_>>(), vec![RuleSetId(3)]);
        assert_eq!(sets.remove(RuleSetId(3)), Ok(true));
        assert!(!sets.contains(RuleSetId(3)));
    }
}
