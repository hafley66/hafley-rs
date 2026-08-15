//! Facts as relations: one enum, one `to_dl6`/`parse_dl6` round trip. These are
//! the DL6 spellings the plan pins (`ARCH.pl`): ingest facts asserted by the
//! pipe from boop rows, agent behavior declarations authored by the user, and
//! state facts the fold maintains. For v1 this is plain Rust with a stable text
//! form; the one-to-one DL6 mapping is preserved so the swap later is a loader
//! change, not a pipeline change.

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};

/// A single relation fact. Variants are the DL6 relation names in section 4 of
/// the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    Turn {
        session: String,
        turn: i64,
        role: String,
        said: String,
        ts: i64,
    },
    Pair {
        session: String,
        turn: i64,
        ai_text: String,
        user_text: String,
    },
    Stale {
        session: String,
        turn: i64,
    },
    Agent {
        id: String,
    },
    OnRequest {
        agent: String,
        request: String,
        action: String,
    },
    Policy {
        agent: String,
        remind_every: i64,
        remind_cap: i64,
        bundle: i64,
    },
    StateNote {
        agent: String,
        key: String,
        body: String,
    },
    StateEdge {
        agent: String,
        from: String,
        to: String,
        kind: String,
    },
}

impl Fact {
    /// The relation name, the first token in `rel name(...)`.
    pub fn relation(&self) -> &'static str {
        match self {
            Fact::Turn { .. } => "turn",
            Fact::Pair { .. } => "pair",
            Fact::Stale { .. } => "stale",
            Fact::Agent { .. } => "agent",
            Fact::OnRequest { .. } => "on_request",
            Fact::Policy { .. } => "policy",
            Fact::StateNote { .. } => "state_note",
            Fact::StateEdge { .. } => "state_edge",
        }
    }

    /// Render as one DL6 line: `fact turn(session="s", turn=1, ...).`
    pub fn to_dl6(&self) -> String {
        let args = match self {
            Fact::Turn {
                session,
                turn,
                role,
                said,
                ts,
            } => vec![
                kv("session", session),
                kv("turn", &turn.to_string()),
                kv("role", role),
                kv("said", said),
                kv("ts", &ts.to_string()),
            ],
            Fact::Pair {
                session,
                turn,
                ai_text,
                user_text,
            } => vec![
                kv("session", session),
                kv("turn", &turn.to_string()),
                kv("ai_text", ai_text),
                kv("user_text", user_text),
            ],
            Fact::Stale { session, turn } => {
                vec![kv("session", session), kv("turn", &turn.to_string())]
            }
            Fact::Agent { id } => vec![kv("id", id)],
            Fact::OnRequest {
                agent,
                request,
                action,
            } => vec![
                kv("agent", agent),
                kv("request", request),
                kv("action", action),
            ],
            Fact::Policy {
                agent,
                remind_every,
                remind_cap,
                bundle,
            } => vec![
                kv("agent", agent),
                kv("remind_every", &remind_every.to_string()),
                kv("remind_cap", &remind_cap.to_string()),
                kv("bundle", &bundle.to_string()),
            ],
            Fact::StateNote { agent, key, body } => {
                vec![kv("agent", agent), kv("key", key), kv("body", body)]
            }
            Fact::StateEdge {
                agent,
                from,
                to,
                kind,
            } => vec![
                kv("agent", agent),
                kv("from", from),
                kv("to", to),
                kv("kind", kind),
            ],
        };
        format!("fact {}({}).", self.relation(), args.join(", "))
    }
}

fn kv(name: &str, value: &str) -> String {
    format!("{name}={}", quote(value))
}

/// Quote a value as a double-quoted DL6 string literal, escaping `"` and `\`.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Parse one DL6 line into a `Fact`. Accepts lines emitted by `to_dl6`; the
/// parser is deliberately narrow (v1 reads back its own output, not arbitrary
/// DL6). Unknown relations error instead of silently dropping state.
pub fn parse_dl6(line: &str) -> Result<Fact> {
    let line = line.trim();
    let line = line
        .strip_prefix("fact ")
        .ok_or_else(|| anyhow!("state line must start with `fact `: {line:?}"))?;
    let line = line
        .strip_suffix('.')
        .ok_or_else(|| anyhow!("state line must end with `.`: {line:?}"))?;
    let open = line
        .find('(')
        .ok_or_else(|| anyhow!("state line missing `(`: {line:?}"))?;
    let relation = &line[..open];
    let args = &line[open + 1..];
    let mut map = BTreeMap::new();
    for (name, value) in split_args(args)? {
        map.insert(name.to_owned(), value.to_owned());
    }
    let get = |name: &str| -> Result<&str> {
        map.get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("relation {relation} missing `{name}`"))
    };
    let get_int = |name: &str| -> Result<i64> {
        get(name)?
            .parse()
            .with_context(|| format!("relation {relation} field `{name}` not an integer"))
    };
    Ok(match relation {
        "turn" => Fact::Turn {
            session: get("session")?.to_owned(),
            turn: get_int("turn")?,
            role: get("role")?.to_owned(),
            said: get("said")?.to_owned(),
            ts: get_int("ts")?,
        },
        "pair" => Fact::Pair {
            session: get("session")?.to_owned(),
            turn: get_int("turn")?,
            ai_text: get("ai_text")?.to_owned(),
            user_text: get("user_text")?.to_owned(),
        },
        "stale" => Fact::Stale {
            session: get("session")?.to_owned(),
            turn: get_int("turn")?,
        },
        "agent" => Fact::Agent {
            id: get("id")?.to_owned(),
        },
        "on_request" => Fact::OnRequest {
            agent: get("agent")?.to_owned(),
            request: get("request")?.to_owned(),
            action: get("action")?.to_owned(),
        },
        "policy" => Fact::Policy {
            agent: get("agent")?.to_owned(),
            remind_every: get_int("remind_every")?,
            remind_cap: get_int("remind_cap")?,
            bundle: get_int("bundle")?,
        },
        "state_note" => Fact::StateNote {
            agent: get("agent")?.to_owned(),
            key: get("key")?.to_owned(),
            body: get("body")?.to_owned(),
        },
        "state_edge" => Fact::StateEdge {
            agent: get("agent")?.to_owned(),
            from: get("from")?.to_owned(),
            to: get("to")?.to_owned(),
            kind: get("kind")?.to_owned(),
        },
        other => bail!("unknown relation `{other}`"),
    })
}

/// Split `name=value, name=value` honoring quoted values containing commas.
/// The `args` slice is the inside of `(...)`, so a trailing `)` terminates the
/// final argument.
fn split_args(args: &str) -> Result<Vec<(&str, String)>> {
    let mut out = Vec::new();
    let mut rest = args.trim();
    while !rest.is_empty() && !rest.starts_with(')') {
        let eq = rest
            .find('=')
            .ok_or_else(|| anyhow!("arg not name=value: {rest:?}"))?;
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
            let end = end.ok_or_else(|| anyhow!("unterminated quoted value: {value:?}"))?;
            out.push((name, unescape(&quoted[..end])));
            rest = &quoted[end + 1..];
        } else {
            let value_end = value
                .find([',', ')'])
                .unwrap_or(value.len());
            let raw = &value[..value_end];
            out.push((name, raw.trim().to_owned()));
            rest = &value[value_end..];
        }
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix(',') {
            rest = after.trim_start();
        } else if rest.is_empty() || rest.starts_with(')') {
            break;
        } else {
            return Err(anyhow!("expected `,` after arg, got {rest:?}"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{parse_dl6, Fact};

    fn assert_roundtrip(fact: Fact) {
        let text = fact.to_dl6();
        let parsed = parse_dl6(&text).unwrap();
        assert_eq!(parsed, fact, "round trip failed for {text}");
    }

    #[test]
    fn round_trips_a_turn_fact() {
        assert_roundtrip(Fact::Turn {
            session: "s1".into(),
            turn: 3,
            role: "user".into(),
            said: "rewrite this".into(),
            ts: 1723,
        });
    }

    #[test]
    fn round_trips_every_variant() {
        assert_roundtrip(Fact::Pair {
            session: "s".into(),
            turn: 1,
            ai_text: "a".into(),
            user_text: "u".into(),
        });
        assert_roundtrip(Fact::Stale {
            session: "s".into(),
            turn: 1,
        });
        assert_roundtrip(Fact::Agent { id: "tighten".into() });
        assert_roundtrip(Fact::OnRequest {
            agent: "tighten".into(),
            request: "rewrite".into(),
            action: "send(template=tighten, remind=2/8)".into(),
        });
        assert_roundtrip(Fact::Policy {
            agent: "tighten".into(),
            remind_every: 2,
            remind_cap: 8,
            bundle: 1,
        });
        assert_roundtrip(Fact::StateNote {
            agent: "tighten".into(),
            key: "intro".into(),
            body: "body".into(),
        });
        assert_roundtrip(Fact::StateEdge {
            agent: "tighten".into(),
            from: "a".into(),
            to: "b".into(),
            kind: "spawned".into(),
        });
    }

    #[test]
    fn quoted_values_may_contain_commas_and_quotes() {
        let fact = Fact::StateNote {
            agent: "tighten".into(),
            key: "a,b\"c".into(),
            body: "line one\nline two".into(),
        };
        assert_roundtrip(fact);
    }

    #[test]
    fn unknown_relation_is_an_error_not_a_drop() {
        assert!(parse_dl6("fact mystery(x=1).").is_err());
    }
}
