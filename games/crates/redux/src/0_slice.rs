//! Generic reducer algebra: one shape for phases, screens, and systems.
//!
//! `(state, event, context) -> output`, with effects escaping only as inert descriptors through
//! the sink. Implementations and compositions are zero-sized types, so pipelines monomorphize to
//! straight-line code while all rollback state remains in the caller-owned `State`.

use core::marker::PhantomData;

/// One reducer / one phase. `Output` is the same-tick value handed to the next slice; `Effect` is
/// a zero-or-more descriptor channel for a runner outside rollback state.
pub trait Slice {
    type Context<'a>: Copy;
    type State;
    type Event;
    type Output;
    type Effect;
    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output;
}

/// Uninhabited effect: `Effect = Never` is a compile-time proof that a slice is pure.
pub enum Never {}

/// Chaining: A then B, where `A::Output` feeds `B::Event` and state/context/effect are shared.
pub struct Then<A, B>(PhantomData<(A, B)>);

impl<A, B> Slice for Then<A, B>
where
    A: Slice,
    B: for<'a> Slice<
            State = A::State,
            Context<'a> = A::Context<'a>,
            Event = A::Output,
            Effect = A::Effect,
        >,
{
    type Context<'a> = A::Context<'a>;
    type State = A::State;
    type Event = A::Event;
    type Output = B::Output;
    type Effect = A::Effect;
    #[inline(always)]
    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        let mid = A::reduce(st, ev, cx, fx);
        B::reduce(st, mid, cx, fx)
    }
}

/// Lift one slice's effect into a wider effect enum so different effect vocabularies compose.
pub struct MapEffect<A, F>(PhantomData<(A, F)>);

impl<A, F> Slice for MapEffect<A, F>
where
    A: Slice,
    F: From<A::Effect>,
{
    type Context<'a> = A::Context<'a>;
    type State = A::State;
    type Event = A::Event;
    type Output = A::Output;
    type Effect = F;
    #[inline(always)]
    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        A::reduce(st, ev, cx, &mut |effect| fx(F::from(effect)))
    }
}

/// Structural focus from `Outer` onto one inner state value.
pub trait Lens<Outer, Inner>: Copy {
    fn get(outer: &Outer) -> &Inner;
    fn get_mut(outer: &mut Outer) -> &mut Inner;
}

/// Run `Inner` against a lens-selected sub-state of `Outer`.
pub struct Zoom<LensZ, Inner, Outer>(PhantomData<(LensZ, Inner, Outer)>);

impl<LensZ, Inner, Outer> Slice for Zoom<LensZ, Inner, Outer>
where
    Inner: Slice,
    LensZ: Lens<Outer, Inner::State>,
{
    type Context<'a> = Inner::Context<'a>;
    type State = Outer;
    type Event = Inner::Event;
    type Output = Inner::Output;
    type Effect = Inner::Effect;
    #[inline(always)]
    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        Inner::reduce(LensZ::get_mut(st), ev, cx, fx)
    }
}

/// Declare a zero-sized slice and its `Slice` implementation. Omitting `effect` selects `Never`.
#[macro_export]
macro_rules! slice {
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident for $state:ty {
            context: $cx:ty, event: $ev:ty, output: $out:ty, effect: $eff:ty,
            reduce($state_id:ident, $ev_id:ident, $cx_id:ident, $fx_id:ident) $body:block
        }
    ) => {
        $(#[$meta])*
        $vis struct $name;
        impl $crate::Slice for $name {
            type Context<'a> = $cx;
            type State = $state;
            type Event = $ev;
            type Output = $out;
            type Effect = $eff;
            #[inline(always)]
            fn reduce(
                $state_id: &mut Self::State,
                $ev_id: Self::Event,
                $cx_id: Self::Context<'_>,
                $fx_id: &mut impl ::core::ops::FnMut(Self::Effect),
            ) -> Self::Output $body
        }
        const _: () = ::core::assert!(::core::mem::size_of::<$name>() == 0);
    };
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident for $state:ty {
            context: $cx:ty, event: $ev:ty, output: $out:ty,
            reduce($state_id:ident, $ev_id:ident, $cx_id:ident, $fx_id:ident) $body:block
        }
    ) => {
        $crate::slice! {
            $(#[$meta])*
            $vis $name for $state {
                context: $cx, event: $ev, output: $out, effect: $crate::Never,
                reduce($state_id, $ev_id, $cx_id, $fx_id) $body
            }
        }
    };
}

macro_rules! then_chain {
    ($only:ty $(,)?) => { $only };
    ($head:ty, $($tail:ty),+ $(,)?) => {
        $crate::Then<$head, then_chain!($($tail),+)>
    };
}

macro_rules! slice_tuple_impl {
    ($($ty:ident),+ $(,)?) => {
        impl<$($ty),+> $crate::Slice for ($($ty,)+)
        where
            then_chain!($($ty),+): $crate::Slice,
        {
            type Context<'a> = <then_chain!($($ty),+) as $crate::Slice>::Context<'a>;
            type State = <then_chain!($($ty),+) as $crate::Slice>::State;
            type Event = <then_chain!($($ty),+) as $crate::Slice>::Event;
            type Output = <then_chain!($($ty),+) as $crate::Slice>::Output;
            type Effect = <then_chain!($($ty),+) as $crate::Slice>::Effect;
            #[inline(always)]
            fn reduce(
                st: &mut Self::State,
                ev: Self::Event,
                cx: Self::Context<'_>,
                fx: &mut impl FnMut(Self::Effect),
            ) -> Self::Output {
                <then_chain!($($ty),+) as $crate::Slice>::reduce(st, ev, cx, fx)
            }
        }
    };
}

macro_rules! slice_tuple_impls {
    () => {
        slice_tuple_impl!(A, B);
        slice_tuple_impl!(A, B, C);
        slice_tuple_impl!(A, B, C, D);
        slice_tuple_impl!(A, B, C, D, E);
        slice_tuple_impl!(A, B, C, D, E, F);
        slice_tuple_impl!(A, B, C, D, E, F, G);
        slice_tuple_impl!(A, B, C, D, E, F, G, H);
    };
}

slice_tuple_impls!();

/// Per-slot context provider for `Each`.
pub trait EachCx<Inner: Slice, const N: usize>: Copy {
    fn live(&self) -> usize;
    fn idle(&self) -> Inner::Output;
    fn event(&self, slot: usize) -> Inner::Event;
    fn slot_cx(&self, slot: usize) -> Inner::Context<'_>;
}

/// Run `Inner` once per live slot of a fixed state array, collecting a fixed output array.
pub struct Each<Inner, Provider, const N: usize>(PhantomData<(Inner, Provider)>);

impl<Inner, Provider, const N: usize> Slice for Each<Inner, Provider, N>
where
    Inner: Slice,
    Inner::Output: Copy,
    Provider: EachCx<Inner, N>,
{
    type Context<'a> = Provider;
    type State = [Inner::State; N];
    type Event = ();
    type Output = [Inner::Output; N];
    type Effect = Inner::Effect;
    #[inline(always)]
    fn reduce(
        st: &mut Self::State,
        _ev: Self::Event,
        cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        let live = cx.live();
        let mut out = [cx.idle(); N];
        for slot in 0..live {
            out[slot] = Inner::reduce(&mut st[slot], cx.event(slot), cx.slot_cx(slot), fx);
        }
        out
    }
}
