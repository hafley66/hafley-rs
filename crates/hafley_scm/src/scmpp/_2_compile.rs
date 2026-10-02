use tree_sitter::{Language, Query};

use super::_0_types::{CapRef, Compiled, Cond, FlatPattern, Level, Rel, Rows, ScmppError, Walk};
use super::_1_parens::{split, Arg, Pred};

pub const ROOT: &str = "__root";

struct Ctx<'a> {
    lang: &'a Language,
    patterns: Vec<FlatPattern>,
}

/// Enclosing capture tables, outermost first; index = `CapRef.level`.
type Scope<'s> = &'s [Vec<Box<str>>];

pub fn compile(lang: &Language, text: &str) -> Result<Compiled, ScmppError> {
    let mut ctx = Ctx {
        lang,
        patterns: Vec::new(),
    };
    let plan = level(&mut ctx, text, 0, &[])?;
    super::_3_lower::exports(&plan, &ctx.patterns)?;
    let sql = super::_3_lower::lower(&plan, &ctx.patterns);
    let match_sql = super::_3_lower::lower_matches(&plan, &ctx.patterns);
    Ok(Compiled {
        patterns: ctx.patterns,
        plan,
        sql,
        match_sql,
    })
}

fn unsupported(pred: &Pred, message: &str) -> ScmppError {
    ScmppError::Unsupported(format!("#{} at byte {}: {message}", pred.op, pred.offset))
}

fn resolve(name: &str, mine: &[Box<str>], outer: Scope) -> Option<CapRef> {
    let depth = outer.len();
    std::iter::once((depth, mine))
        .chain(
            outer
                .iter()
                .enumerate()
                .rev()
                .map(|(d, names)| (d, names.as_slice())),
        )
        .find(|(_, names)| names.iter().any(|seen| seen.as_ref() == name))
        .map(|(d, _)| CapRef {
            level: d as u8,
            name: name.into(),
        })
}

pub(super) const TEXT_BUILTINS: [&str; 13] = [
    "eq?",
    "not-eq?",
    "any-eq?",
    "any-not-eq?",
    "match?",
    "not-match?",
    "any-match?",
    "any-not-match?",
    "any-of?",
    "not-any-of?",
    "set!",
    "is?",
    "is-not?",
];

fn level(ctx: &mut Ctx, text: &str, base: usize, outer: Scope) -> Result<Level, ScmppError> {
    let split = split(text, base)?;
    let mine = split.captures;
    if mine.iter().any(|name| name.as_ref() == ROOT) {
        return Err(ScmppError::Unsupported(format!("@{ROOT} is reserved")));
    }
    let depth = outer.len() as u8;
    let mut conds = Vec::new();
    for name in &mine {
        if let Some(enclosing) = resolve(name, &[], outer) {
            conds.push(Cond::Same(
                CapRef {
                    level: depth,
                    name: name.clone(),
                },
                enclosing,
            ));
        }
    }
    let mut kept = String::new();
    let mut relations = Vec::new();
    for pred in &split.preds {
        let captured = |name: &str| {
            resolve(name, &mine, outer)
                .ok_or_else(|| unsupported(pred, &format!("unknown capture @{name}")))
        };
        let (negated, bare) = pred
            .op
            .strip_prefix("not-")
            .map_or((false, pred.op.as_str()), |rest| (true, rest));
        if TEXT_BUILTINS.contains(&pred.op.as_str()) {
            let names: Vec<&str> = pred
                .args
                .iter()
                .filter_map(|arg| match arg {
                    Arg::Capture(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            for name in &names {
                captured(name)?;
            }
            if names
                .iter()
                .all(|name| mine.iter().any(|seen| seen.as_ref() == *name))
            {
                kept.push(' ');
                kept.push_str(&pred.text);
                continue;
            }
            let cond = match (bare, pred.args.as_slice()) {
                ("eq?", [Arg::Capture(a), Arg::Capture(b)]) => {
                    Cond::TextEq(captured(a)?, captured(b)?)
                }
                ("match?", [Arg::Capture(a), Arg::Str(pattern)]) => {
                    Cond::TextMatch(captured(a)?, pattern.as_str().into())
                }
                _ => {
                    return Err(unsupported(
                        pred,
                        "across levels only #eq? @a @b and #match? @a \"re\"",
                    ))
                }
            };
            conds.push(if negated {
                Cond::Not(Box::new(cond))
            } else {
                cond
            });
            continue;
        }
        match bare {
            "contains?" => {
                let [Arg::Capture(name), literals @ ..] = pred.args.as_slice() else {
                    return Err(unsupported(pred, "expects @capture \"literal\"+"));
                };
                let literals = literals
                    .iter()
                    .map(|arg| match arg {
                        Arg::Str(text) | Arg::Word(text) => Ok(text.as_str().into()),
                        _ => Err(unsupported(pred, "expects @capture \"literal\"+")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if literals.is_empty() {
                    return Err(unsupported(pred, "expects @capture \"literal\"+"));
                }
                let cond = Cond::Contains(captured(name)?, literals);
                conds.push(if negated {
                    Cond::Not(Box::new(cond))
                } else {
                    cond
                });
            }
            "has?" | "has-ancestor?" | "has-parent?" | "precedes?" | "follows?" | "nth-child?" => {
                relations.push((pred, negated, bare));
            }
            _ => return Err(unsupported(pred, "unknown predicate")),
        }
    }
    let pattern = ctx.patterns.len() as u16;
    let flat = format!("({} @{ROOT}{kept})", split.root);
    let query = {
        #[cfg(feature = "shared")]
        let _span = tracing::trace_span!("scmpp_query_new").entered();
        Query::new(ctx.lang, &flat)
    }
    .map_err(|error| ScmppError::Query {
        pattern,
        text: flat.clone(),
        error,
    })?;
    let captures = query
        .capture_names()
        .iter()
        .filter(|name| **name != ROOT)
        .map(|name| (*name).into())
        .collect();
    ctx.patterns.push(FlatPattern {
        id: pattern,
        text: flat,
        captures,
        query,
    });
    let mut inner = outer.to_vec();
    inner.push(mine.clone());
    let mut rels = Vec::new();
    for (pred, negated, bare) in relations {
        rels.push(relation(ctx, pred, negated, bare, &mine, outer, &inner)?);
    }
    Ok(Level {
        pattern,
        rels,
        conds,
    })
}

/// `kind+` positional targets become one alternation level; supertypes expand inside tree-sitter.
fn kinds_text(kinds: &[String]) -> String {
    let one = |kind: &String| {
        if kind.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            format!("({kind})")
        } else {
            format!("\"{}\"", kind.replace('\\', "\\\\").replace('"', "\\\""))
        }
    };
    match kinds {
        [kind] => one(kind),
        _ => format!("[{}]", kinds.iter().map(one).collect::<Vec<_>>().join(" ")),
    }
}

fn relation(
    ctx: &mut Ctx,
    pred: &Pred,
    negated: bool,
    bare: &str,
    mine: &[Box<str>],
    outer: Scope,
    inner: Scope,
) -> Result<Rel, ScmppError> {
    let [Arg::Capture(from), args @ ..] = pred.args.as_slice() else {
        return Err(unsupported(
            pred,
            "first argument is the @capture the relation starts at",
        ));
    };
    let from = resolve(from, mine, outer)
        .ok_or_else(|| unsupported(pred, &format!("unknown capture @{from}")))?;
    let mut rel = Rel {
        from,
        walk: Walk::Descendant,
        neighbor: false,
        stop: None,
        field: None,
        rows: Rows::First,
        negated,
        target: None,
    };
    let mut args = args;
    if bare == "nth-child?" {
        let [Arg::Word(index), rest @ ..] = args else {
            return Err(unsupported(pred, "expects @capture N [of L]"));
        };
        let index = index
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| unsupported(pred, "N is a positive integer"))?;
        rel.walk = Walk::NthChild(index);
        args = match rest {
            [] => rest,
            [Arg::Word(of) | Arg::Key(of), rest @ ..] if of == "of" && !rest.is_empty() => rest,
            _ => return Err(unsupported(pred, "expects @capture N [of L]")),
        };
        let target = target_text(pred, &mut args)?;
        if !args.is_empty() {
            return Err(unsupported(pred, "nth-child takes no options"));
        }
        if let Some((text, base)) = target {
            rel.target = Some(Box::new(level(ctx, &text, base, &[])?));
        }
        return Ok(rel);
    }
    rel.walk = match bare {
        "has?" => Walk::Descendant,
        "has-ancestor?" => Walk::Ancestor,
        "has-parent?" => Walk::Parent,
        "precedes?" => Walk::Precedes,
        _ => Walk::Follows,
    };
    let (text, base) = target_text(pred, &mut args)?
        .ok_or_else(|| unsupported(pred, "needs a (pattern) or kind+"))?;
    let mut seen: Vec<&str> = Vec::new();
    let mut stop = None;
    while let [Arg::Key(key), value, rest @ ..] = args {
        if seen.contains(&key.as_str()) {
            return Err(unsupported(pred, &format!("duplicate option {key}:")));
        }
        seen.push(key);
        match (key.as_str(), value) {
            ("stopBy", Arg::Word(word)) if word == "neighbor" => rel.neighbor = true,
            ("stopBy", Arg::Word(word)) if word == "end" => {}
            ("stopBy", Arg::Group(text, base)) => stop = Some((text, *base)),
            ("field", Arg::Word(name)) => {
                if ctx.lang.field_id_for_name(name).is_none() {
                    return Err(unsupported(pred, &format!("unknown field {name}")));
                }
                rel.field = Some(name.as_str().into());
            }
            ("rows", Arg::Word(word)) if word == "first" => rel.rows = Rows::First,
            ("rows", Arg::Word(word)) if word == "each" => rel.rows = Rows::Each,
            _ => return Err(unsupported(pred, &format!("bad option {key}: {value:?}"))),
        }
        args = rest;
    }
    if !args.is_empty() {
        return Err(unsupported(
            pred,
            &format!("unexpected argument {:?}", args[0]),
        ));
    }
    if rel.walk == Walk::Parent && (rel.neighbor || stop.is_some()) {
        return Err(unsupported(
            pred,
            "has-parent is one step; stopBy does not apply",
        ));
    }
    rel.target = Some(Box::new(level(ctx, &text, base, inner)?));
    if let Some((text, base)) = stop {
        rel.stop = Some(Box::new(level(ctx, text, base, &[])?));
    }
    Ok(rel)
}

/// One `(pattern)` group or a run of kind words; consumed from the front of `args`.
fn target_text(pred: &Pred, args: &mut &[Arg]) -> Result<Option<(String, usize)>, ScmppError> {
    if let [Arg::Group(text, base), rest @ ..] = *args {
        *args = rest;
        return Ok(Some((text.clone(), *base)));
    }
    let mut kinds = Vec::new();
    while let [Arg::Word(kind) | Arg::Str(kind), rest @ ..] = *args {
        kinds.push(kind.clone());
        *args = rest;
    }
    Ok((!kinds.is_empty()).then(|| (kinds_text(&kinds), pred.offset)))
}
