use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionObservation {
    pub observation_key: String,
    pub session_id: String,
    pub observed_at_ms: u64,
    pub harness: Option<String>,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    pub parent_pid: Option<u32>,
    pub pane_id: Option<String>,
    pub tui_session_id: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRelation {
    pub relation_key: String,
    pub from_session: String,
    pub to_session: String,
    pub kind: SessionRelationKind,
    pub source: String,
    pub observed_at_ms: u64,
    pub matched_identity_key: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SessionRelationKind {
    ContinuedIn,
    SameProcess,
    ParentChild,
    SamePane,
    SameTui,
}

impl SessionRelationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContinuedIn => "continued-in",
            Self::SameProcess => "same-process",
            Self::ParentChild => "parent-child",
            Self::SamePane => "same-pane",
            Self::SameTui => "same-tui",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "continued-in" => Some(Self::ContinuedIn),
            "same-process" => Some(Self::SameProcess),
            "parent-child" => Some(Self::ParentChild),
            "same-pane" => Some(Self::SamePane),
            "same-tui" => Some(Self::SameTui),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceJoinLimits {
    pub max_observations: usize,
    pub max_relations: usize,
    pub max_sessions: usize,
}

impl Default for TraceJoinLimits {
    fn default() -> Self {
        Self {
            max_observations: 10_000,
            max_relations: 10_000,
            max_sessions: 10_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedTrace {
    pub sessions: Vec<String>,
    pub root_session: String,
    pub evidence: Vec<SessionRelation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceJoinDiagnostic {
    ObservationBudgetExceeded {
        limit: usize,
        observed: usize,
    },
    RelationBudgetExceeded {
        limit: usize,
        observed: usize,
    },
    SessionBudgetExceeded {
        limit: usize,
        observed: usize,
    },
    AmbiguousRelation {
        session: String,
        kind: SessionRelationKind,
    },
}

impl std::fmt::Display for TraceJoinDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ObservationBudgetExceeded { limit, observed } => write!(
                formatter,
                "trace join observation budget exceeded: {observed} > {limit}"
            ),
            Self::RelationBudgetExceeded { limit, observed } => write!(
                formatter,
                "trace join relation budget exceeded: {observed} > {limit}"
            ),
            Self::SessionBudgetExceeded { limit, observed } => write!(
                formatter,
                "trace join session budget exceeded: {observed} > {limit}"
            ),
            Self::AmbiguousRelation { session, kind } => write!(
                formatter,
                "trace join has multiple {:?} endpoints for session {session}",
                kind
            ),
        }
    }
}

impl std::error::Error for TraceJoinDiagnostic {}

pub fn derive_traces(
    observations: &[SessionObservation],
    relations: &[SessionRelation],
    limits: TraceJoinLimits,
) -> Result<Vec<DerivedTrace>, TraceJoinDiagnostic> {
    if observations.len() > limits.max_observations {
        return Err(TraceJoinDiagnostic::ObservationBudgetExceeded {
            limit: limits.max_observations,
            observed: observations.len(),
        });
    }
    if relations.len() > limits.max_relations {
        return Err(TraceJoinDiagnostic::RelationBudgetExceeded {
            limit: limits.max_relations,
            observed: relations.len(),
        });
    }

    let mut sessions = observations
        .iter()
        .map(|observation| observation.session_id.clone())
        .collect::<BTreeSet<_>>();
    for relation in relations {
        sessions.insert(relation.from_session.clone());
        sessions.insert(relation.to_session.clone());
    }
    if sessions.len() > limits.max_sessions {
        return Err(TraceJoinDiagnostic::SessionBudgetExceeded {
            limit: limits.max_sessions,
            observed: sessions.len(),
        });
    }

    let mut incoming = BTreeMap::<(String, SessionRelationKind), BTreeSet<String>>::new();
    let mut outgoing = BTreeMap::<(String, SessionRelationKind), BTreeSet<String>>::new();
    for relation in relations {
        incoming
            .entry((relation.to_session.clone(), relation.kind))
            .or_default()
            .insert(relation.from_session.clone());
        outgoing
            .entry((relation.from_session.clone(), relation.kind))
            .or_default()
            .insert(relation.to_session.clone());
    }
    for ((session, kind), endpoints) in incoming.iter().chain(outgoing.iter()) {
        if *kind == SessionRelationKind::ContinuedIn && endpoints.len() > 1 {
            return Err(TraceJoinDiagnostic::AmbiguousRelation {
                session: session.clone(),
                kind: *kind,
            });
        }
    }

    let mut adjacency = BTreeMap::<String, Vec<(String, usize)>>::new();
    for (index, relation) in relations.iter().enumerate() {
        adjacency
            .entry(relation.from_session.clone())
            .or_default()
            .push((relation.to_session.clone(), index));
        adjacency
            .entry(relation.to_session.clone())
            .or_default()
            .push((relation.from_session.clone(), index));
    }

    let mut earliest = BTreeMap::<String, u64>::new();
    for observation in observations {
        earliest
            .entry(observation.session_id.clone())
            .and_modify(|seen| *seen = (*seen).min(observation.observed_at_ms))
            .or_insert(observation.observed_at_ms);
    }
    let mut unseen = sessions;
    let mut traces = Vec::new();
    while let Some(seed) = unseen.pop_first() {
        let mut queue = VecDeque::from([seed]);
        let mut component = BTreeSet::new();
        let mut evidence_indexes = BTreeSet::new();
        while let Some(session) = queue.pop_front() {
            if !component.insert(session.clone()) {
                continue;
            }
            if component.len() > limits.max_sessions {
                return Err(TraceJoinDiagnostic::SessionBudgetExceeded {
                    limit: limits.max_sessions,
                    observed: component.len(),
                });
            }
            if let Some(neighbors) = adjacency.get(&session) {
                for (neighbor, edge_index) in neighbors {
                    evidence_indexes.insert(*edge_index);
                    if !component.contains(neighbor) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        for session in &component {
            unseen.remove(session);
        }
        let root_session = component
            .iter()
            .min_by_key(|session| {
                (
                    earliest.get(*session).copied().unwrap_or(u64::MAX),
                    *session,
                )
            })
            .cloned()
            .unwrap_or_default();
        traces.push(DerivedTrace {
            sessions: component.into_iter().collect(),
            root_session,
            evidence: evidence_indexes
                .into_iter()
                .map(|index| relations[index].clone())
                .collect(),
        });
    }
    traces.sort_by(|left, right| left.root_session.cmp(&right.root_session));
    Ok(traces)
}

#[cfg(test)]
mod tests {
    use super::{
        derive_traces, SessionObservation, SessionRelation, SessionRelationKind,
        TraceJoinDiagnostic, TraceJoinLimits,
    };

    fn observation(session_id: &str, observed_at_ms: u64, cwd: &str) -> SessionObservation {
        SessionObservation {
            observation_key: format!("test:{session_id}:{observed_at_ms}"),
            session_id: session_id.into(),
            observed_at_ms,
            harness: Some("claude".into()),
            cwd: Some(cwd.into()),
            pid: None,
            parent_pid: None,
            pane_id: None,
            tui_session_id: None,
            source: "test".into(),
        }
    }

    fn continued(from: &str, to: &str) -> SessionRelation {
        SessionRelation {
            relation_key: format!("continued-in:{from}:{to}"),
            from_session: from.into(),
            to_session: to.into(),
            kind: SessionRelationKind::ContinuedIn,
            source: "claude-transcript".into(),
            observed_at_ms: 1,
            matched_identity_key: None,
        }
    }

    #[test]
    fn six_session_continuation_chain_ignores_cwd_proximity() {
        let observations = (0..7)
            .map(|index| observation(&format!("session-{index}"), index, "/repo"))
            .collect::<Vec<_>>();
        let relations = (0..5)
            .map(|index| {
                continued(
                    &format!("session-{index}"),
                    &format!("session-{}", index + 1),
                )
            })
            .collect::<Vec<_>>();
        let traces = derive_traces(&observations, &relations, TraceJoinLimits::default()).unwrap();
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].root_session, "session-0");
        assert_eq!(traces[0].sessions.len(), 6);
        assert_eq!(traces[1].sessions, vec!["session-6"]);
    }

    #[test]
    fn repeated_continuation_endpoints_return_ambiguity() {
        let relations = vec![continued("old-a", "new"), continued("old-b", "new")];
        assert_eq!(
            derive_traces(&[], &relations, TraceJoinLimits::default()).unwrap_err(),
            TraceJoinDiagnostic::AmbiguousRelation {
                session: "new".into(),
                kind: SessionRelationKind::ContinuedIn,
            }
        );
    }

    #[test]
    fn join_limits_name_the_exceeded_input() {
        let observations = vec![observation("session", 1, "/repo")];
        assert_eq!(
            derive_traces(
                &observations,
                &[],
                TraceJoinLimits {
                    max_observations: 0,
                    ..TraceJoinLimits::default()
                }
            )
            .unwrap_err(),
            TraceJoinDiagnostic::ObservationBudgetExceeded {
                limit: 0,
                observed: 1,
            }
        );
        assert_eq!(
            derive_traces(
                &[],
                &[continued("a", "b")],
                TraceJoinLimits {
                    max_relations: 0,
                    ..TraceJoinLimits::default()
                }
            )
            .unwrap_err(),
            TraceJoinDiagnostic::RelationBudgetExceeded {
                limit: 0,
                observed: 1,
            }
        );
        assert_eq!(
            derive_traces(
                &[],
                &[continued("a", "b")],
                TraceJoinLimits {
                    max_sessions: 1,
                    ..TraceJoinLimits::default()
                }
            )
            .unwrap_err(),
            TraceJoinDiagnostic::SessionBudgetExceeded {
                limit: 1,
                observed: 2,
            }
        );
    }
}
