//! Reduction and effect settlement at a borrow-safe execution boundary.

use crate::Slice;

/// Reduce one event, then apply the inert effects produced by that reduction.
///
/// `effects` is caller-owned scratch storage. It is cleared before reduction and drained
/// after reduction returns, so its capacity can be reused by the next call. The reducer has
/// completed before `apply` receives the first effect, which permits `apply` to mutate the
/// same state that was reduced. Any allocation needed to grow the buffer remains visible to
/// the caller through the supplied `Vec`.
pub fn reduce_then_apply<'a, R, Apply>(
    state: &mut R::State,
    event: R::Event,
    cx: R::Context<'a>,
    effects: &mut Vec<R::Effect>,
    mut apply: Apply,
) -> R::Output
where
    R: Slice,
    Apply: FnMut(&mut R::State, R::Effect),
{
    effects.clear();
    let output = R::reduce(state, event, cx, &mut |effect| effects.push(effect));
    for effect in effects.drain(..) {
        apply(state, effect);
    }
    output
}
