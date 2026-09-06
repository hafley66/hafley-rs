use crate::{DebugFrame, DeclarationTrace, GravityScale, Property};
use std::panic::Location;

/// Match-owned declarations with one typed column and its selector index.
///
/// ```
/// use cascade::{GravityScale, Rules, GRAVITY_SCALE};
/// let mut rules = Rules::<()>::default();
/// rules.set(GRAVITY_SCALE, GravityScale(1.0));
/// ```
///
/// ```compile_fail,E0308
/// use cascade::{Rules, GRAVITY_SCALE};
/// let mut rules = Rules::<()>::default();
/// rules.set(GRAVITY_SCALE, [0.0, 0.0]);
/// ```
pub struct Rules<S> {
    gravity: Vec<DeclarationTrace>,
    selectors: Vec<fn(&S, u32) -> bool>,
}

impl<S> Default for Rules<S> {
    fn default() -> Self {
        Self {
            gravity: Vec::new(),
            selectors: Vec::new(),
        }
    }
}

impl<S> Rules<S> {
    #[track_caller]
    pub fn set(&mut self, key: Property<GravityScale>, value: GravityScale) {
        self.when(|_, _| true, (0, 0), key, value);
    }

    #[track_caller]
    pub fn when(
        &mut self,
        selector: fn(&S, u32) -> bool,
        precedence: (u16, u16),
        _key: Property<GravityScale>,
        value: GravityScale,
    ) {
        self.gravity.push(DeclarationTrace {
            source_order: self.gravity.len(),
            precedence,
            value,
            location: Location::caller(),
        });
        self.selectors.push(selector);
    }

    pub fn resolve(&self, snapshot: &S, entity: u32, key: Property<GravityScale>) -> DebugFrame {
        let mut matching: Vec<_> = self
            .gravity
            .iter()
            .filter(|declaration| (self.selectors[declaration.source_order])(snapshot, entity))
            .copied()
            .collect();
        matching.sort_by_key(|declaration| (declaration.precedence, declaration.source_order));
        DebugFrame {
            entity,
            property: key.id,
            winner: matching.last().copied(),
            matching,
        }
    }
}
