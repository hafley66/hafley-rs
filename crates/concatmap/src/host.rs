//! The evaluator boundary. The `Dl6Host` trait is the shape the plan pins
//! (section 4): assert facts, evaluate rules, return actions not side effects.
//! `Host` is the v1 concrete implementation over plain structs; when the DL6
//! engine lands it swaps in behind the same trait with no pipeline change.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::action::Action;
use crate::fact::Fact;
use crate::rules::{classify_request, Dispatch, ReminderBudget, RuleSet};
use crate::state::{fold_reply, State};

/// One user/assistant exchange to route. Built from the store's turn rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub session: String,
    pub turn: i64,
    pub ai_text: String,
    pub user_text: String,
}

/// The evaluator contract. `assert` ingests facts (turns, pairs, state, or the
/// user's rule facts); `evaluate` returns actions for one dispatch step.
pub trait Dl6Host {
    fn assert(&mut self, fact: Fact) -> Result<()>;
    fn evaluate(&mut self) -> Result<Vec<Action>>;
}

/// The v1 `Dl6Host` implementation.
pub struct Host {
    pub agent: String,
    rules: RuleSet,
    state: State,
    state_path: PathBuf,
    pending: VecDeque<Pair>,
    in_flight: Option<Pair>,
    done: BTreeSet<(String, i64)>,
    budget: ReminderBudget,
    /// Set when the state file changed and needs a commit.
    dirty: bool,
}

impl Host {
    /// `state_path` is the `<agent>.dl6` file; a missing file loads as empty.
    pub fn new(agent: &str, rules: RuleSet, state_path: PathBuf) -> Result<Host> {
        let state = State::load(&state_path)
            .with_context(|| format!("load state for agent {agent}"))?;
        let budget = ReminderBudget::new(rules.policy(agent));
        Ok(Host {
            agent: agent.to_owned(),
            rules,
            state,
            state_path,
            pending: VecDeque::new(),
            in_flight: None,
            done: BTreeSet::new(),
            budget,
            dirty: false,
        })
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_path(&self) -> &PathBuf {
        &self.state_path
    }

    /// Enqueue a pair. A pair already dispatched is a no-op (idempotent). A
    /// pair arriving while a reply is in flight cancels the in-flight pair so
    /// the newest proceeds (coalesce-to-newest / switchScan semantics).
    pub fn ingest_pair(&mut self, pair: Pair) {
        let key = (pair.session.clone(), pair.turn);
        if self.done.contains(&key) || self.pending.contains(&pair) {
            return;
        }
        if self.in_flight.is_some() {
            // Cancel in-flight, newest proceeds: drop the awaiting pair and
            // put the newcomer at the head of the queue.
            self.in_flight = None;
            self.pending.push_front(pair);
            return;
        }
        self.pending.push_back(pair);
    }

    /// Fold a completed model reply into state, returning the fold actions and
    /// a commit when the state actually changed. An idempotent replay (the same
    /// facts already present) does not dirty the state.
    pub fn complete_reply(&mut self, reply: &str) -> Vec<Action> {
        let fold = fold_reply(&self.agent, reply, &self.state);
        let mut actions = Vec::new();
        let mut changed = false;
        for fact in &fold.asserts {
            changed |= self.state.apply(fact, false);
            actions.push(Action::Assert(fact.clone()));
        }
        for fact in &fold.retracts {
            changed |= self.state.apply(fact, true);
            actions.push(Action::Retract(fact.clone()));
        }
        if changed {
            self.dirty = true;
        }
        self.in_flight = None;
        actions
    }

    fn dispatch_pair(&mut self, pair: Pair) -> Result<Vec<Action>> {
        let request = classify_request(&pair.user_text);
        let mut actions = Vec::new();
        match self.rules.dispatch(&self.agent, &request) {
            Ok(Dispatch::Skip) => actions.push(Action::Skip),
            Ok(Dispatch::Send(spec)) => {
                self.in_flight = Some(pair.clone());
                let vars = {
                    let mut vars = std::collections::BTreeMap::new();
                    vars.insert("session".to_owned(), pair.session.clone());
                    vars.insert("turn".to_owned(), pair.turn.to_string());
                    vars.insert("ai_text".to_owned(), pair.ai_text.clone());
                    vars.insert("user_text".to_owned(), pair.user_text.clone());
                    vars
                };
                actions.push(Action::Send {
                    template: spec.template,
                    vars,
                });
                if self.budget.step() {
                    actions.push(Action::Remind {
                        text: "reminder".to_owned(),
                    });
                }
            }
            Err(error) => {
                // A request with no route is skipped, but loud.
                tracing::warn!(agent = self.agent, request, "{error}");
                actions.push(Action::Skip);
            }
        }
        Ok(actions)
    }
}

impl Dl6Host for Host {
    fn assert(&mut self, fact: Fact) -> Result<()> {
        match fact {
            Fact::Pair {
                session,
                turn,
                ai_text,
                user_text,
            } => {
                self.ingest_pair(Pair {
                    session,
                    turn,
                    ai_text,
                    user_text,
                });
                Ok(())
            }
            Fact::StateNote { agent, key, body } => {
                self.state.apply(&Fact::StateNote { agent, key, body }, false);
                self.dirty = true;
                Ok(())
            }
            Fact::StateEdge { agent, from, to, kind } => {
                self.state
                    .apply(&Fact::StateEdge { agent, from, to, kind }, false);
                self.dirty = true;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn evaluate(&mut self) -> Result<Vec<Action>> {
        if self.in_flight.is_some() {
            return Ok(Vec::new());
        }
        loop {
            let Some(pair) = self.pending.pop_front() else {
                return Ok(Vec::new());
            };
            let key = (pair.session.clone(), pair.turn);
            if self.done.contains(&key) {
                continue;
            }
            self.done.insert(key);
            return self.dispatch_pair(pair);
        }
    }
}

impl Host {
    /// Whether the state changed since the last flush.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Write the state file when dirty, clearing the flag. Returns whether the
    /// file was written (true means a commit is warranted).
    pub fn flush(&mut self) -> Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        self.state.save(&self.state_path)?;
        self.dirty = false;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::action::Action;
    use crate::fact::Fact;
    use crate::host::{Dl6Host, Host, Pair};
    use crate::rules::RuleSet;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("concatmap_host_{}_{}", std::process::id(), name))
    }

    fn pair(session: &str, turn: i64, user: &str) -> Pair {
        Pair {
            session: session.into(),
            turn,
            ai_text: "ai".into(),
            user_text: user.into(),
        }
    }

    fn tighten_rules(remind: (i64, i64)) -> RuleSet {
        let facts = vec![
            Fact::Agent { id: "tighten".into() },
            Fact::OnRequest {
                agent: "tighten".into(),
                request: "rewrite".into(),
                action: "send(template=tighten, remind=2/8)".into(),
            },
            Fact::OnRequest {
                agent: "tighten".into(),
                request: "stale_pair".into(),
                action: "skip".into(),
            },
            Fact::Policy {
                agent: "tighten".into(),
                remind_every: remind.0,
                remind_cap: remind.1,
                bundle: 1,
            },
        ];
        RuleSet::from_facts(&facts)
    }

    #[test]
    fn rules_route_a_request_to_one_send() {
        let mut host = Host::new(
            "tighten",
            tighten_rules((0, 0)),
            temp_path("route"),
        )
        .unwrap();
        host.ingest_pair(pair("s", 1, "please rewrite this"));
        let actions = host.evaluate().unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Send { template, .. } => assert_eq!(template, "tighten"),
            other => panic!("expected Send, got {other:?}"),
        }
        let _ = std::fs::remove_file(temp_path("route"));
    }

    #[test]
    fn stale_pair_rule_skips() {
        let mut host = Host::new("tighten", tighten_rules((0, 0)), temp_path("stale")).unwrap();
        host.ingest_pair(pair("s", 1, "pair is stale"));
        let actions = host.evaluate().unwrap();
        assert_eq!(actions, vec![Action::Skip]);
        let _ = std::fs::remove_file(temp_path("stale"));
    }

    #[test]
    fn reminder_cadence_is_a_fact_not_code() {
        let mut host = Host::new("tighten", tighten_rules((2, 2)), temp_path("remind")).unwrap();
        let mut seen = Vec::new();
        for turn in 1..=4 {
            host.ingest_pair(pair("s", turn, "rewrite this"));
            let actions = host.evaluate().unwrap();
            seen.push(actions.iter().any(|a| matches!(a, Action::Remind { .. })));
            host.complete_reply("fact state_note(agent=\"tighten\", key=\"k\", body=\"x\")");
        }
        // Reminders on dispatches 2 and 4 only, then silent.
        assert_eq!(seen, vec![false, true, false, true]);
        let _ = std::fs::remove_file(temp_path("remind"));
    }

    #[test]
    fn a_pair_in_flight_is_cancelled_by_a_newer_pair() {
        let mut host = Host::new("tighten", tighten_rules((0, 0)), temp_path("inflight")).unwrap();
        host.ingest_pair(pair("s", 1, "rewrite this"));
        let first = host.evaluate().unwrap();
        assert_eq!(first.len(), 1);
        // A second pair arrives while the first reply is in flight.
        host.ingest_pair(pair("s", 2, "rewrite this too"));
        let actions = host.evaluate().unwrap();
        assert_eq!(actions.len(), 1, "newest pair proceeds with a Send");
        assert!(matches!(&actions[0], Action::Send { .. }));
        let _ = std::fs::remove_file(temp_path("inflight"));
    }

    #[test]
    fn replaying_a_done_pair_does_not_re_dispatch() {
        let mut host = Host::new("tighten", tighten_rules((0, 0)), temp_path("replay")).unwrap();
        host.ingest_pair(pair("s", 1, "rewrite this"));
        let first = host.evaluate().unwrap();
        assert_eq!(first.len(), 1);
        host.complete_reply("no directives");
        host.ingest_pair(pair("s", 1, "rewrite this"));
        assert!(host.evaluate().unwrap().is_empty(), "no second dispatch");
        let _ = std::fs::remove_file(temp_path("replay"));
    }
}
