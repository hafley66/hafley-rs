use std::{marker::PhantomData, panic::Location};

pub type PropertyId = u16;

/// A closed typed key. Public constants are the only key constructors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Property<T> {
    pub(crate) id: PropertyId,
    marker: PhantomData<fn() -> T>,
}

/// Dimensionless multiplier on the caller's gravity acceleration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GravityScale(pub f32);

pub const GRAVITY_SCALE: Property<GravityScale> = Property {
    id: 0,
    marker: PhantomData,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeclarationTrace {
    pub source_order: usize,
    /// Ascending (layer, specificity); insertion order breaks ties.
    pub precedence: (u16, u16),
    pub value: GravityScale,
    pub location: &'static Location<'static>,
}

/// Transient projection. Declaration locations are Rust authoring call sites.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugFrame {
    pub entity: u32,
    pub property: PropertyId,
    pub matching: Vec<DeclarationTrace>,
    pub winner: Option<DeclarationTrace>,
}
