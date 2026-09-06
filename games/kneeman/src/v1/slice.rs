//! Compatibility facade for the generic reducer algebra now owned by the zero-dependency
//! `redux` crate. Existing `crate::v1::slice::*` imports remain source-compatible.

pub use redux::{Each, EachCx, Lens, MapEffect, Never, Slice, Then, Zoom};

#[doc(hidden)]
pub use redux::slice as __redux_slice;

/// Compatibility forwarding macro. New generic consumers may invoke `redux::slice!` directly.
#[macro_export]
macro_rules! slice {
    ($($tokens:tt)*) => {
        $crate::v1::slice::__redux_slice! { $($tokens)* }
    };
}
