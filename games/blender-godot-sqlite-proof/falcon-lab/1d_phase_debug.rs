//! Lab-local debug observer for the live Falcon locomotion phase.
//!
//! The authoritative value is `game_fighter::State.phase`, projected as
//! `FrameValues.phase`/`frame_values_phase` in the existing presentation row.
//! Animation `action`/`pose` are a separate concept and are not used here.
//!
//! `game_fighter::State` owns only the current phase and `phase_tick`; it does
//! not retain the previous phase, because that would duplicate rollback history.
//! The debug view observes the presented stream and derives the previous phase,
//! the transition tick and the observed-edge set. The observed-edge set is
//! explicitly *not* a complete legal-edge graph: it only contains edges this
//! run has actually crossed.

use std::collections::BTreeMap;

/// Names indexed by `game_fighter::Phase` discriminant. Keep in sync with
/// `motion_phase_code` in `0a_live_values.rs`.
pub const PHASE_NAMES: [&str; 12] = [
    "IDLE", "WALK", "DASH", "RUN", "BRAKE", "TURN", "SQUAT", "CROUCH", "LANDING", "JUMP", "FALL",
    "AIRJUMP",
];

/// Name for a projected phase code, or `"NONE"` when no movement state exists.
pub fn phase_name(code: f64) -> &'static str {
    if !code.is_finite() || code.fract() != 0.0 || code < 0.0 || code as usize >= PHASE_NAMES.len() {
        return "NONE";
    }
    PHASE_NAMES[code as usize]
}

/// One observed phase change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    /// Tick the new phase was entered, derived from `tick - phase_ticks + 1`.
    pub tick: i64,
    pub from: u8,
    pub to: u8,
}

/// Stateful observer fed one presented frame per tick by the control path or a
/// test. Deterministic and headless.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhaseDebug {
    active: Option<u8>,
    prev: Option<u8>,
    enter_tick: i64,
    last_tick: Option<i64>,
    edges: BTreeMap<(u8, u8), u64>,
    transitions: u64,
}

impl PhaseDebug {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop all observed history. Used on rollback rewind.
    pub fn reset(&mut self) {
        self.active = None;
        self.prev = None;
        self.enter_tick = 0;
        self.last_tick = None;
        self.edges.clear();
        self.transitions = 0;
    }

    pub fn active(&self) -> Option<u8> {
        self.active
    }

    pub fn enter_tick(&self) -> i64 {
        self.enter_tick
    }

    pub fn transitions(&self) -> u64 {
        self.transitions
    }

    /// Observed transition counts keyed by `(from, to)`. Observed only.
    pub fn observed_edges(&self) -> &BTreeMap<(u8, u8), u64> {
        &self.edges
    }

    /// The active phase is the highlighted node in any rendered graph.
    pub fn highlighted_node(&self) -> Option<u8> {
        self.active
    }

    /// Feed one frame. `phase` is `FrameValues.phase` (-1 when absent) and
    /// `phase_ticks` is `FrameValues.phase_ticks`. Returns the observed edge
    /// when the active phase changed, else `None`.
    ///
    /// A tick earlier than the previous observation is a rollback rewind: all
    /// observed history is cleared so a replayed prefix cannot fabricate edges
    /// or resurrect a stale previous phase.
    pub fn observe(&mut self, tick: i64, phase: f64, phase_ticks: f64) -> Option<Transition> {
        if self.last_tick.is_some_and(|last| tick <= last) {
            if self.last_tick == Some(tick) && self.active.map(f64::from) == Some(phase) {
                return None;
            }
            self.reset();
        }
        self.last_tick = Some(tick);
        if phase_name(phase) == "NONE" {
            self.reset();
            return None;
        }
        let now = phase as u8;
        let entry = if phase_ticks.is_finite() && phase_ticks >= 1.0 {
            tick - (phase_ticks as i64) + 1
        } else {
            tick
        };
        match self.active {
            Some(prev) if prev == now => None,
            _ => {
                let from = self.active;
                self.active = Some(now);
                self.prev = from;
                self.enter_tick = entry;
                self.transitions += 1;
                if let Some(from) = from {
                    *self.edges.entry((from, now)).or_insert(0) += 1;
                    return Some(Transition {
                        tick: entry,
                        from,
                        to: now,
                    });
                }
                None
            }
        }
    }

    /// "PHASE RUN / PREV BRAKE / T84 [OBSERVED]".
    pub fn active_label(&self) -> String {
        match self.active {
            Some(now) => {
                let prev =
                    self.prev.map_or("NONE".to_string(), |p| phase_name(f64::from(p)).to_string());
                format!(
                    "PHASE {} / PREV {} / T{} [OBSERVED]",
                    phase_name(f64::from(now)),
                    prev,
                    self.enter_tick
                )
            }
            None => "PHASE NONE [OBSERVED]".into(),
        }
    }

    /// "OBSERVED EDGES (not a legal-edge graph): IDLE->DASH, DASH->RUN".
    pub fn observed_edge_label(&self) -> String {
        if self.edges.is_empty() {
            return "OBSERVED EDGES (not a legal-edge graph): NONE".into();
        }
        let edges = self
            .edges
            .keys()
            .map(|(from, to)| {
                format!(
                    "{}->{}",
                    phase_name(f64::from(*from)),
                    phase_name(f64::from(*to))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("OBSERVED EDGES (not a legal-edge graph): {edges}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: f64 = 0.0;
    const DASH: f64 = 2.0;
    const RUN: f64 = 3.0;
    const BRAKE: f64 = 4.0;

    #[test]
    fn boundary_crossings_report_prev_enter_tick_and_highlight() {
        let mut dbg = PhaseDebug::new();
        // Frame with phase_ticks=1 on entry, incrementing while held.
        assert_eq!(dbg.observe(10, IDLE, 1.0), None);
        assert_eq!(dbg.highlighted_node(), Some(0));
        assert_eq!(
            dbg.observe(11, DASH, 1.0),
            Some(Transition { tick: 11, from: 0, to: 2 })
        );
        assert_eq!(dbg.observe(12, DASH, 2.0), None);
        assert_eq!(
            dbg.observe(20, RUN, 1.0),
            Some(Transition { tick: 20, from: 2, to: 3 })
        );
        // Entered RUN 4 ticks ago at tick 20, still RUN at 23.
        assert_eq!(dbg.observe(23, RUN, 4.0), None);
        assert_eq!(
            dbg.observe(30, BRAKE, 1.0),
            Some(Transition { tick: 30, from: 3, to: 4 })
        );
        assert_eq!(dbg.highlighted_node(), Some(4));
        assert_eq!(dbg.enter_tick(), 30);
        assert_eq!(dbg.transitions(), 4);
        assert_eq!(
            dbg.observed_edges().keys().copied().collect::<Vec<_>>(),
            vec![(0, 2), (2, 3), (3, 4)]
        );
    }

    #[test]
    fn absent_phase_is_ignored_and_edges_are_labelled_observed() {
        let mut dbg = PhaseDebug::new();
        assert_eq!(dbg.observe(0, -1.0, -1.0), None);
        assert_eq!(dbg.observe(1, f64::NAN, f64::NAN), None);
        assert_eq!(dbg.active(), None);
        assert_eq!(dbg.observed_edge_label(), "OBSERVED EDGES (not a legal-edge graph): NONE");
        dbg.observe(2, IDLE, 1.0);
        dbg.observe(3, DASH, 1.0);
        assert!(dbg.observed_edge_label().starts_with("OBSERVED EDGES (not a legal-edge graph):"));
        assert!(dbg.observed_edge_label().contains("IDLE->DASH"));
        assert!(dbg.active_label().contains("PHASE DASH"));
        assert!(dbg.active_label().contains("PREV IDLE"));
        assert!(dbg.active_label().contains("[OBSERVED]"));
    }

    #[test]
    fn enter_tick_is_stable_for_mid_phase_observation_start() {
        // Observer joins 5 ticks into RUN: entry = tick - phase_ticks + 1.
        let mut dbg = PhaseDebug::new();
        assert_eq!(dbg.observe(42, RUN, 5.0), None);
        assert_eq!(dbg.enter_tick(), 38);
    }

    #[test]
    fn tick_rewind_clears_observed_history_without_fabricating_edges() {
        let mut dbg = PhaseDebug::new();
        dbg.observe(10, IDLE, 1.0);
        dbg.observe(11, DASH, 1.0);
        dbg.observe(12, RUN, 1.0);
        assert_eq!(dbg.transitions(), 3);
        assert_eq!(dbg.observed_edges().keys().copied().collect::<Vec<_>>(), vec![(0, 2), (2, 3)]);
        // Rollback to tick 9 and replay: history is discarded, no edge emitted
        // from the stale RUN state, and the same replayed edge appears once.
        assert_eq!(dbg.observe(9, IDLE, 1.0), None);
        assert_eq!(dbg.transitions(), 1);
        assert!(dbg.observed_edges().is_empty());
        assert_eq!(dbg.active(), Some(0));
        assert_eq!(dbg.observe(10, DASH, 1.0), Some(Transition { tick: 10, from: 0, to: 2 }));
        assert_eq!(dbg.observe(11, RUN, 1.0), Some(Transition { tick: 11, from: 2, to: 3 }));
        assert_eq!(
            dbg.observed_edges().values().copied().collect::<Vec<_>>(),
            vec![1, 1],
            "each replayed edge is observed exactly once"
        );
    }

    #[test]
    fn repeated_tick_and_invalid_codes_cannot_create_edges() {
        for invalid in [-1.0, 12.0, 0.5, f64::NAN, f64::INFINITY] {
            let mut dbg = PhaseDebug::new();
            dbg.observe(10, IDLE, 1.0);
            dbg.observe(11, DASH, 1.0);
            let before = dbg.clone();
            assert_eq!(dbg.observe(11, DASH, 1.0), None);
            assert_eq!(dbg, before);
            assert_eq!(dbg.observe(11, RUN, 1.0), None);
            assert!(dbg.observed_edges().is_empty());
            dbg.observe(12, invalid, 1.0);
            assert_eq!(dbg.active(), None);
            assert_eq!(phase_name(invalid), "NONE");
        }
    }
}
