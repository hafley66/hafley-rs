//! The pipe's state: one `.dl6` file per agent holding only state facts. The
//! interpreter applies asserts and retractions here; the git commit history of
//! this file is the analysis corpus. The file is pure facts, never fenced
//! render output; rendering is derived.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};

use crate::fact::{parse_dl6, Fact};

/// The in-memory form of one agent's state file.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// `(agent, key)` -> body, from `state_note` facts.
    pub notes: BTreeMap<(String, String), String>,
    /// `(agent, from, to, kind)` from `state_edge` facts.
    pub edges: BTreeSet<(String, String, String, String)>,
}

impl State {
    /// Load state from a `.dl6` file. A missing file is an empty state, not an
    /// error. A present line that is not a state fact errors, so a corrupt or
    /// foreign state file is loud.
    pub fn load(path: &Path) -> Result<State> {
        let mut state = State::default();
        if !path.exists() {
            return Ok(state);
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read state file {}", path.display()))?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('%') {
                continue;
            }
            let fact = parse_dl6(line).with_context(|| format!("in {}", path.display()))?;
            match fact {
                Fact::StateNote { agent, key, body } => {
                    state.notes.insert((agent, key), body);
                }
                Fact::StateEdge {
                    agent,
                    from,
                    to,
                    kind,
                } => {
                    state.edges.insert((agent, from, to, kind));
                }
                other => {
                    anyhow::bail!(
                        "state file {} holds a non-state fact `{}`",
                        path.display(),
                        other.relation()
                    )
                }
            }
        }
        Ok(state)
    }

    /// Apply one fold action. `retract=true` removes a matching fact; otherwise
    /// it inserts. Returns whether the state actually changed (a no-op
    /// re-insert of an identical fact is false, so an idempotent fold stays
    /// clean). Missing matches on retract are no-ops that return false.
    pub fn apply(&mut self, fact: &Fact, retract: bool) -> bool {
        match fact {
            Fact::StateNote { agent, key, body } => {
                if retract {
                    self.notes
                        .remove(&(agent.clone(), key.clone()))
                        .is_some()
                } else {
                    let existing = self.notes.get(&(agent.clone(), key.clone()));
                    if existing == Some(body) {
                        false
                    } else {
                        self.notes
                            .insert((agent.clone(), key.clone()), body.clone());
                        true
                    }
                }
            }
            Fact::StateEdge {
                agent,
                from,
                to,
                kind,
            } => {
                let edge = (agent.clone(), from.clone(), to.clone(), kind.clone());
                if retract {
                    self.edges.remove(&edge)
                } else {
                    self.edges.insert(edge)
                }
            }
            _ => false,
        }
    }

    /// Render the state back to `.dl6` facts, deterministic order.
    pub fn to_dl6(&self) -> String {
        let mut lines = Vec::new();
        for ((agent, key), body) in &self.notes {
            lines.push(
                Fact::StateNote {
                    agent: agent.clone(),
                    key: key.clone(),
                    body: body.clone(),
                }
                .to_dl6(),
            );
        }
        for (agent, from, to, kind) in &self.edges {
            lines.push(
                Fact::StateEdge {
                    agent: agent.clone(),
                    from: from.clone(),
                    to: to.clone(),
                    kind: kind.clone(),
                }
                .to_dl6(),
            );
        }
        lines.join("\n")
    }

    /// Write the rendered state to `path` (truncating), creating the parent
    /// directory if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create state dir {}", parent.display()))?;
        }
        std::fs::write(path, self.to_dl6())
            .with_context(|| format!("write state file {}", path.display()))
    }
}

/// The outcome of folding one model reply into state: facts to assert and facts
/// to retract. Appends alone cannot correct state, so the fold honors explicit
/// retractions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fold {
    pub asserts: Vec<Fact>,
    pub retracts: Vec<Fact>,
}

/// Fold a model reply into state operations. v1 scans the reply for embedded
/// directives of the form `fact state_note(...)`/`fact state_edge(...)` to
/// assert and `retract state_note(...)`/`retract state_edge(...)` to retract.
/// Lines that are not directives are ignored.
pub fn fold_reply(reply: &str, state: &State) -> Fold {
    let mut fold = Fold::default();
    for line in reply.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("fact ") {
            if let Ok(fact) = parse_directive(rest) {
                fold.asserts.push(fact);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("retract ") {
            if let Ok(fact) = parse_directive(rest) {
                // Only retract state facts that actually exist; otherwise the
                // fold is a no-op and produces no spurious commit.
                let present = match &fact {
                    Fact::StateNote { agent, key, .. } => {
                        state.notes.contains_key(&(agent.clone(), key.clone()))
                    }
                    Fact::StateEdge { agent, from, to, kind } => state.edges.contains(&(
                        agent.clone(),
                        from.clone(),
                        to.clone(),
                        kind.clone(),
                    )),
                    _ => false,
                };
                if present {
                    fold.retracts.push(fact);
                }
            }
        }
    }
    fold
}

/// Parse an inline directive body (`state_note(...)`) as a fact. A directive
/// inside a model reply may or may not carry the trailing `.` of a persisted
/// fact line; both are accepted.
fn parse_directive(body: &str) -> Result<Fact> {
    let body = body.trim();
    let text = if body.ends_with(')') {
        format!("fact {body}.")
    } else {
        format!("fact {body}")
    };
    parse_dl6(&text)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::fact::Fact;
    use crate::state::{fold_reply, State};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "concatmap_state_{}_{}",
            std::process::id(),
            name
        ))
    }

    fn note(key: &str, body: &str) -> Fact {
        Fact::StateNote {
            agent: "tighten".into(),
            key: key.into(),
            body: body.into(),
        }
    }

    #[test]
    fn missing_file_is_empty_state() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let state = State::load(&path).unwrap();
        assert!(state.notes.is_empty());
    }

    #[test]
    fn load_save_round_trips() {
        let path = temp_path("roundtrip");
        let mut state = State::default();
        state.apply(&note("intro", "hi"), false);
        state.save(&path).unwrap();
        let loaded = State::load(&path).unwrap();
        assert_eq!(loaded.notes.get(&("tighten".into(), "intro".into())).unwrap(), "hi");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn retract_removes_a_key_and_missing_retract_is_a_noop() {
        let mut state = State::default();
        state.apply(&note("a", "1"), false);
        state.apply(&note("a", "1"), true);
        assert!(state.notes.is_empty());
        // Retracting a key that is absent must not error or create anything.
        state.apply(&note("b", "2"), true);
        assert!(state.notes.is_empty());
    }

    #[test]
    fn fold_picks_up_asserts_and_existing_retractions() {
        let mut state = State::default();
        state.apply(&note("k", "old"), false);
        let reply = concat!(
            "some prose\n",
            "fact state_note(agent=\"tighten\", key=\"k\", body=\"new\")\n",
            "retract state_note(agent=\"tighten\", key=\"k\", body=\"old\")\n",
            "retract state_note(agent=\"tighten\", key=\"absent\", body=\"x\")\n",
        );
        let fold = fold_reply(reply, &state);
        assert_eq!(fold.asserts.len(), 1);
        assert_eq!(fold.retracts.len(), 1, "absent key retraction is dropped");
        let _ = std::fs::remove_file(temp_path("fold"));
    }
}
