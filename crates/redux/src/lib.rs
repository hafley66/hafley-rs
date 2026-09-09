//! Literal redux. The whole idea in one line: **a Store is `scan(reducer)` plus a subject.**
//!
//! - The reducer is a pure function `(&State, &Action) -> State`. Same actions in, same
//!   state out, on every machine, every replay. No I/O, no clock, no globals.
//! - `dispatch` is one scan step: fold the action into the state, then notify subscribers.
//! - `subscribe` is the subject half: listeners see each new state (a render pass, a
//!   logger, a persistence hook), and structurally CANNOT dispatch re-entrantly — the
//!   store is mutably borrowed while they run, so the borrow checker enforces what
//!   redux.js checks at runtime.
//!
//! What is deliberately NOT here: middleware, thunks, combineReducers, observables.
//! Rust already has the pieces — a struct of sub-states with a `match` in the reducer IS
//! `combineReducers`; effects are values your reducer returns in `State` (or an effect
//! sink you thread yourself); async lives outside the store entirely. Keeping those out
//! is what keeps a rollback netcode layer able to treat the reducer as a hot loop: a
//! rollback session is just this same reducer scanned N frames at a time over a
//! speculative input window, committing confirmed frames into a downstream store.
//!
//! Determinism contract (what makes a reducer rollback-grade, not just tidy):
//! - `fn` pointer, not a closure: the reducer captures nothing. Config rides inside the
//!   action or the state, where replays can see it.
//! - `State` should be plain data (`Clone`, ideally `Copy` + `serde`): snapshots are
//!   assignments, checksums are byte-folds.
//! - No wall clock, no thread timing, no HashMap iteration order in the reducer.

#[path = "1_pool.rs"]
mod pool;
#[path = "0_slice.rs"]
mod slice;

pub use pool::{FreeSpans, Handle, Pool};
pub use slice::{Each, EachCx, Lens, MapEffect, Never, Slice, Then, Zoom};

/// Handle returned by [`Store::subscribe`]; pass to [`Store::unsubscribe`] to detach.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Sub(u64);

/// `scan(reducer)` + subject. See the crate docs for the one-line mental model.
pub struct Store<S, A> {
    state: S,
    reducer: fn(&S, &A) -> S,
    #[allow(clippy::type_complexity)]
    subs: Vec<(u64, Box<dyn FnMut(&S)>)>,
    next_sub: u64,
}

impl<S, A> Store<S, A> {
    pub fn new(reducer: fn(&S, &A) -> S, initial: S) -> Self {
        Self {
            state: initial,
            reducer,
            subs: Vec::new(),
            next_sub: 0,
        }
    }

    /// The current state. Reads are free; there is no other way to observe the store
    /// besides subscribing.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// One scan step: `state = reducer(&state, &action)`, then every subscriber sees the
    /// new state. Returns the new state for call-site convenience.
    pub fn dispatch(&mut self, action: A) -> &S {
        self.state = (self.reducer)(&self.state, &action);
        for (_, f) in &mut self.subs {
            f(&self.state);
        }
        &self.state
    }

    /// Attach a listener to the subject half. It runs after every dispatch, in
    /// subscription order. It receives `&S` only — no store access, so no re-entrant
    /// dispatch (enforced by the borrow checker, not a runtime flag).
    pub fn subscribe(&mut self, f: impl FnMut(&S) + 'static) -> Sub {
        let id = self.next_sub;
        self.next_sub += 1;
        self.subs.push((id, Box::new(f)));
        Sub(id)
    }

    pub fn unsubscribe(&mut self, sub: Sub) {
        self.subs.retain(|(id, _)| *id != sub.0);
    }

    /// Swap the reducer (redux's `replaceReducer`): hot-reload, code-split, or lift a
    /// store into a wider action vocabulary. State is untouched.
    pub fn replace_reducer(&mut self, reducer: fn(&S, &A) -> S) {
        self.reducer = reducer;
    }
}

/// The store without the subject: fold a whole action log through the reducer. This is
/// replay, save-file loading, and the determinism test harness — one function, because a
/// pure reducer makes them all the same operation.
pub fn replay<S, A>(
    reducer: fn(&S, &A) -> S,
    initial: S,
    actions: impl IntoIterator<Item = A>,
) -> S {
    actions.into_iter().fold(initial, |s, a| reducer(&s, &a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Copy, Clone, PartialEq, Debug)]
    enum Act {
        Add(i32),
        Reset,
    }

    fn counter(s: &i32, a: &Act) -> i32 {
        match a {
            Act::Add(n) => s + n,
            Act::Reset => 0,
        }
    }

    #[test]
    fn dispatch_is_one_scan_step() {
        let mut st = Store::new(counter, 0);
        st.dispatch(Act::Add(2));
        st.dispatch(Act::Add(3));
        assert_eq!(*st.state(), 5);
        st.dispatch(Act::Reset);
        assert_eq!(*st.state(), 0);
    }

    #[test]
    fn subscribers_see_every_new_state_until_unsubscribed() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut st = Store::new(counter, 0);
        let tap = {
            let seen = seen.clone();
            st.subscribe(move |s| seen.borrow_mut().push(*s))
        };
        st.dispatch(Act::Add(1));
        st.dispatch(Act::Add(1));
        st.unsubscribe(tap);
        st.dispatch(Act::Add(1));
        assert_eq!(*seen.borrow(), vec![1, 2], "detached before the third");
    }

    #[test]
    fn replay_equals_live_dispatch() {
        // determinism in one assert: folding the log = running the store.
        let log = [Act::Add(4), Act::Add(-1), Act::Reset, Act::Add(9)];
        let mut live = Store::new(counter, 0);
        for a in log {
            live.dispatch(a);
        }
        assert_eq!(replay(counter, 0, log), *live.state());
    }

    #[test]
    fn replace_reducer_keeps_state() {
        fn doubler(s: &i32, a: &Act) -> i32 {
            match a {
                Act::Add(n) => s + n * 2,
                Act::Reset => 0,
            }
        }
        let mut st = Store::new(counter, 0);
        st.dispatch(Act::Add(3));
        st.replace_reducer(doubler);
        st.dispatch(Act::Add(3));
        assert_eq!(*st.state(), 9, "state survived, new math applied");
    }
}
