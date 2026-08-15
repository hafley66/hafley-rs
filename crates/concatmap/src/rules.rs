//! The user-authored behavior declarations and the dispatch over them: which
//! agent answers which request with which template, reminder cadence, and
//! bundling. This is the v1 plain-Rust stand-in for DL6 rules; each struct maps
//! one-to-one onto a relation in section 4 of the plan.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::fact::Fact;

/// A policy budget: reminder cadence and bundling size, from the `policy`
/// relation. `remind_every=0` or `remind_cap=0` disables reminders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    pub remind_every: i64,
    pub remind_cap: i64,
    pub bundle: i64,
}

impl Policy {
    pub fn from_fact(fact: &Fact) -> Option<Policy> {
        match fact {
            Fact::Policy {
                agent: _,
                remind_every,
                remind_cap,
                bundle,
            } => Some(Policy {
                remind_every: *remind_every,
                remind_cap: *remind_cap,
                bundle: *bundle,
            }),
            _ => None,
        }
    }
}

/// The dispatch decision for one pair: what to do with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Skip,
    Send(SendSpec),
}

/// A parsed `send(template=..., remind=A/B)` action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendSpec {
    pub template: String,
    pub remind_every: i64,
    pub remind_cap: i64,
}

/// Parse an `on_request` action string into a `Dispatch`.
///
/// `"skip"` maps to `Dispatch::Skip`; `"send(template=X, remind=A/B)"` maps to
/// `Dispatch::Send`. Anything else errors so a typo in a rule file is loud,
/// never silently skipped.
pub fn parse_action(action: &str) -> Result<Dispatch> {
    if action == "skip" {
        return Ok(Dispatch::Skip);
    }
    let inner = action
        .strip_prefix("send(")
        .and_then(|rest| rest.strip_suffix(')'))
        .with_context(|| format!("action is neither `skip` nor `send(...)`: {action:?}"))?;
    let mut map = BTreeMap::new();
    for (name, value) in parse_keyvals(inner)? {
        map.insert(name.to_owned(), value.to_owned());
    }
    let template = map
        .get("template")
        .cloned()
        .ok_or_else(|| anyhow!("send action missing `template`: {action:?}"))?;
    let (remind_every, remind_cap) = match map.get("remind") {
        Some(remind) => parse_remind(remind)
            .with_context(|| format!("send action has a bad remind: {action:?}"))?,
        None => (0, 0),
    };
    Ok(Dispatch::Send(SendSpec {
        template,
        remind_every,
        remind_cap,
    }))
}

fn parse_remind(value: &str) -> Result<(i64, i64)> {
    let (every, cap) = value
        .split_once('/')
        .ok_or_else(|| anyhow!("remind must be `every/cap`, got {value:?}"))?;
    let every = every.parse().context("remind every is not an integer")?;
    let cap = cap.parse().context("remind cap is not an integer")?;
    Ok((every, cap))
}

fn parse_keyvals(inner: &str) -> Result<Vec<(&str, String)>> {
    let mut out = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .ok_or_else(|| anyhow!("keyval not name=value: {rest:?}"))?;
        let name = rest[..eq].trim();
        let value = rest[eq + 1..].trim();
        if let Some(quoted) = value.strip_prefix('"') {
            let mut end = None;
            let mut escaped = false;
            for (idx, ch) in quoted.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    end = Some(idx);
                    break;
                }
            }
            let end = end.ok_or_else(|| anyhow!("unterminated quoted value in {inner:?}"))?;
            let mut raw = String::new();
            let mut chars = quoted[..end].chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some('"') => raw.push('"'),
                        Some('\\') => raw.push('\\'),
                        Some(other) => {
                            raw.push('\\');
                            raw.push(other);
                        }
                        None => raw.push('\\'),
                    }
                } else {
                    raw.push(ch);
                }
            }
            out.push((name, raw));
            rest = quoted[end + 1..].trim();
        } else {
            let value = value
                .split(',')
                .next()
                .ok_or_else(|| anyhow!("missing value for {name}"))?;
            out.push((name, value.trim().to_owned()));
            rest = rest[eq + 1 + value.len()..].trim();
        }
        rest = rest
            .strip_prefix(',')
            .map(str::trim)
            .unwrap_or(rest);
    }
    Ok(out)
}

/// The set of behavior declarations: agents, request routes, and policies.
#[derive(Clone, Debug, Default)]
pub struct RuleSet {
    pub agents: Vec<String>,
    pub routes: Vec<Route>,
    pub policies: BTreeMap<String, Policy>,
}

/// One `on_request(agent, request, action)` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub agent: String,
    pub request: String,
    pub action: String,
}

impl RuleSet {
    /// Fold the agent-behavior facts from a rule file into a `RuleSet`.
    pub fn from_facts(facts: &[Fact]) -> RuleSet {
        let mut agents = Vec::new();
        let mut routes = Vec::new();
        let mut policies = BTreeMap::new();
        for fact in facts {
            match fact {
                Fact::Agent { id } => agents.push(id.clone()),
                Fact::OnRequest {
                    agent,
                    request,
                    action,
                } => routes.push(Route {
                    agent: agent.clone(),
                    request: request.clone(),
                    action: action.clone(),
                }),
                Fact::Policy { agent, .. } => {
                    if let Some(policy) = Policy::from_fact(fact) {
                        policies.insert(agent.clone(), policy);
                    }
                }
                _ => {}
            }
        }
        RuleSet {
            agents,
            routes,
            policies,
        }
    }

    /// The `on_request` action for `(agent, request)`. A missing route is a
    /// `Skip`; a present route parses its action string.
    pub fn dispatch(&self, agent: &str, request: &str) -> Result<Dispatch> {
        let route = self
            .routes
            .iter()
            .find(|route| route.agent == agent && route.request == request)
            .ok_or_else(|| anyhow!("no on_request route for ({agent}, {request})"))?;
        parse_action(&route.action)
    }

    pub fn policy(&self, agent: &str) -> Policy {
        self.policies.get(agent).copied().unwrap_or_default()
    }
}

/// The toml shape of one rule file (v1 loader; the fact/action mapping onto
/// DL6 relations later replaces this with a fact loader).
#[derive(Deserialize)]
struct RuleFile {
    agent: String,
    #[serde(default)]
    route: Vec<RouteToml>,
    #[serde(default)]
    policy: Option<PolicyToml>,
}

#[derive(Deserialize)]
struct RouteToml {
    request: String,
    action: String,
}

#[derive(Deserialize)]
struct PolicyToml {
    remind_every: i64,
    remind_cap: i64,
    bundle: i64,
}

/// Load a `RuleSet` from a toml rule file.
///
/// ```toml
/// agent = "tighten"
/// [[route]]
/// request = "rewrite"
/// action = "send(template=tighten, remind=2/8)"
/// [[route]]
/// request = "stale_pair"
/// action = "skip"
/// [policy]
/// remind_every = 2
/// remind_cap = 8
/// bundle = 1
/// ```
pub fn load_rules(path: &Path) -> Result<RuleSet> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read rule file {}", path.display()))?;
    let file: RuleFile = toml::from_str(&text)
        .with_context(|| format!("parse rule file {}", path.display()))?;
    let mut facts = vec![Fact::Agent { id: file.agent.clone() }];
    for route in &file.route {
        facts.push(Fact::OnRequest {
            agent: file.agent.clone(),
            request: route.request.clone(),
            action: route.action.clone(),
        });
    }
    if let Some(policy) = &file.policy {
        facts.push(Fact::Policy {
            agent: file.agent.clone(),
            remind_every: policy.remind_every,
            remind_cap: policy.remind_cap,
            bundle: policy.bundle,
        });
    }
    Ok(RuleSet::from_facts(&facts))
}

/// Classify a user turn into a request token for `on_request` routing. v1 uses
/// keyword matching over the visible text; the mapping is the only heuristic in
/// the pipe and is the natural place a later DL6 `on_request` rule replaces.
pub fn classify_request(user_text: &str) -> String {
    let text = user_text.to_ascii_lowercase();
    for (keyword, token) in [
        ("rewrite", "rewrite"),
        ("stale", "stale_pair"),
        ("summar", "summarize"),
        ("render", "render"),
    ] {
        if text.contains(keyword) {
            return token.to_owned();
        }
    }
    String::new()
}

/// A reminder budget that is a fact, not code: send a reminder every `every`
/// dispatches, up to `cap` total. `every=0` or `cap=0` never reminds.
#[derive(Clone, Debug, Default)]
pub struct ReminderBudget {
    pub every: i64,
    pub cap: i64,
    sent: i64,
    dispatched: i64,
}

impl ReminderBudget {
    pub fn new(policy: Policy) -> ReminderBudget {
        ReminderBudget {
            every: policy.remind_every,
            cap: policy.remind_cap,
            sent: 0,
            dispatched: 0,
        }
    }

    /// Advance one dispatch and decide whether to emit a reminder.
    pub fn step(&mut self) -> bool {
        self.dispatched += 1;
        let due = self.every > 0 && self.dispatched % self.every == 0 && self.sent < self.cap;
        if due {
            self.sent += 1;
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use crate::rules::{parse_action, classify_request, Dispatch, ReminderBudget, SendSpec};

    #[test]
    fn skip_is_skip() {
        assert_eq!(parse_action("skip").unwrap(), Dispatch::Skip);
    }

    #[test]
    fn send_parses_template_and_remind() {
        assert_eq!(
            parse_action("send(template=tighten, remind=2/8)").unwrap(),
            Dispatch::Send(SendSpec {
                template: "tighten".into(),
                remind_every: 2,
                remind_cap: 8,
            })
        );
    }

    #[test]
    fn send_without_remind_defaults_off() {
        assert_eq!(
            parse_action("send(template=tighten)").unwrap(),
            Dispatch::Send(SendSpec {
                template: "tighten".into(),
                remind_every: 0,
                remind_cap: 0,
            })
        );
    }

    #[test]
    fn a_bad_action_is_an_error() {
        assert!(parse_action("send(").is_err());
        assert!(parse_action("teleport").is_err());
    }

    #[test]
    fn classify_matches_keywords_case_insensitively() {
        assert_eq!(classify_request("please rewrite this"), "rewrite");
        assert_eq!(classify_request("Rewrite it"), "rewrite");
        assert_eq!(classify_request("no match here"), "");
    }

    #[test]
    fn reminder_budget_emits_on_the_cadence_up_to_cap() {
        let mut budget = ReminderBudget::new(crate::rules::Policy {
            remind_every: 2,
            remind_cap: 2,
            bundle: 1,
        });
        assert!(!budget.step());
        assert!(budget.step());
        assert!(!budget.step());
        assert!(budget.step());
        assert!(!budget.step(), "past cap, silent");
    }

    #[test]
    fn zero_policy_never_reminds() {
        let mut budget = ReminderBudget::new(crate::rules::Policy {
            remind_every: 0,
            remind_cap: 0,
            bundle: 1,
        });
        assert!(!budget.step());
        assert!(!budget.step());
    }
}
