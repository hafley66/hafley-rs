use tree_sitter::{Language, Query};

use super::_0_types::{CapRef, Compiled, Cond, FlatPattern, Level, Rel, Rows, ScmppError, Walk};
use super::_1_parens::{cut, split, top_items, Arg, Pred};

pub const ROOT: &str = "__root";

/// Relation predicates; each also has a `not-` form.
const RELATIONS: [&str; 6] = [
    "has?",
    "has-ancestor?",
    "has-parent?",
    "precedes?",
    "follows?",
    "nth-child?",
];

struct Ctx<'a> {
    lang: &'a Language,
    patterns: Vec<FlatPattern>,
    warnings: Vec<String>,
    split: bool,
}

/// Enclosing capture tables, outermost first; index = `CapRef.level`.
type Scope<'s> = &'s [Vec<Box<str>>];

pub fn compile(lang: &Language, text: &str) -> Result<Compiled, ScmppError> {
    compile_with(lang, text, true)
}

/// `split: false` keeps every sibling pattern whole: the split's equivalence oracle in tests.
pub fn compile_unsplit(lang: &Language, text: &str) -> Result<Compiled, ScmppError> {
    compile_with(lang, text, false)
}

fn compile_with(lang: &Language, text: &str, split: bool) -> Result<Compiled, ScmppError> {
    let mut ctx = Ctx {
        lang,
        patterns: Vec::new(),
        warnings: Vec::new(),
        split,
    };
    let plan = level(&mut ctx, text, 0, &[])?;
    super::_3_lower::exports(&plan, &ctx.patterns)?;
    let (captures, fields) = super::_3_lower::dictionaries(&plan, &ctx.patterns);
    let (sql, lists) = super::_3_lower::lower(&plan, &ctx.patterns, &captures, &fields);
    Ok(Compiled {
        patterns: ctx.patterns,
        plan,
        sql,
        captures,
        fields,
        lists,
        warnings: ctx.warnings,
    })
}

/// The first predicate only scm++ evaluates (a relation or `contains?`, plain or `not-`):
/// `(top-level pattern index, op)`.
pub fn first_scmpp_only(text: &str) -> Result<Option<(u16, String)>, ScmppError> {
    for (pattern, item) in top_items(text)?.into_iter().enumerate() {
        let (_, _, preds) = cut(&text[item.clone()], item.start)?;
        if let Some(pred) = preds.iter().find(|pred| {
            let bare = pred.op.strip_prefix("not-").unwrap_or(&pred.op);
            bare == "contains?" || RELATIONS.contains(&bare)
        }) {
            return Ok(Some((pattern as u16, pred.op.clone())));
        }
    }
    Ok(None)
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
            pin: None,
        })
}

const TEXT_BUILTINS: [&str; 13] = [
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
                    pin: None,
                },
                enclosing,
            ));
        }
    }
    let mut kept = String::new();
    let mut kept_preds = Vec::new();
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
                kept_preds.push((pred, names));
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
            bare if RELATIONS.contains(&bare) => {
                relations.push((pred, negated, bare));
            }
            _ => return Err(unsupported(pred, "unknown predicate")),
        }
    }
    let pattern = ctx.patterns.len() as u16;
    let shape = super::_2a_siblings::shape(&split.root, base)?;
    let lowering = match &shape {
        Some(shape) if ctx.split => split_plan(ctx, pattern, shape, &kept_preds)?,
        _ => None,
    };
    let flat = match (&lowering, &shape) {
        (Some(plan), Some(shape)) => format!("(({}) {} @{ROOT}{})", shape.kind, shape.tail, plan.root_preds),
        _ => format!("({} @{ROOT}{kept})", split.root),
    };
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
    let optional = query
        .capture_names()
        .iter()
        .zip(query.capture_quantifiers(0))
        .filter(|(_, quantifier)| matches!(quantifier, tree_sitter::CaptureQuantifier::Zero | tree_sitter::CaptureQuantifier::ZeroOrOne | tree_sitter::CaptureQuantifier::ZeroOrMore))
        .map(|(name, _)| (*name).into())
        .collect();
    ctx.patterns.push(FlatPattern {
        id: pattern,
        text: flat,
        captures,
        optional,
        query,
    });
    let mut inner = outer.to_vec();
    inner.push(mine.clone());
    let mut rels = Vec::new();
    for (pred, negated, bare) in relations {
        rels.push(relation(ctx, pred, negated, bare, &mine, outer, &inner)?);
    }
    // A split child is `has? @__root (child) stopBy: neighbor rows: each`, and each
    // child's root comes after the one before it among the siblings, as in tree-sitter.
    // The children see the root's own captures only; theirs moved down a level.
    let mut this = Level {
        pattern,
        rels,
        conds,
    };
    if let Some(plan) = lowering {
        let mut beside = outer.to_vec();
        beside.push(ctx.patterns[pattern as usize].captures.clone());
        let mut owner: Vec<(Box<str>, u16)> = Vec::new();
        let mut split = Vec::new();
        // Every earlier child precedes every later one, not only its neighbour: the planner can then
        // reach any child from any other by a `sib` range, whichever join its other filters favour.
        let mut previous: Vec<u16> = Vec::new();
        for (field, text) in plan.children {
            let target = level(ctx, &text, base, &beside)?;
            owners(&target, &ctx.patterns, &mut owner);
            for before in &previous {
                this.conds.push(Cond::SiblingOrder(*before, target.pattern));
            }
            previous.push(target.pattern);
            split.push(Rel {
                from: CapRef {
                    level: depth,
                    name: ROOT.into(),
                    pin: None,
                },
                walk: Walk::Descendant,
                neighbor: true,
                stop: None,
                field: field.map(Into::into),
                rows: Rows::Each,
                optional: false,
                negated: false,
                target: Some(Box::new(target)),
            });
        }
        for (negated, a, b) in plan.pinned {
            let at = |name: Box<str>| CapRef { level: depth, name, pin: None };
            let cond = Cond::TextEq(at(a), at(b));
            this.conds.push(if negated { Cond::Not(Box::new(cond)) } else { cond });
        }
        // Every reference to a moved capture, in this level's conditions, its relations and their
        // targets, names the child that owns it; the children lower first so their joins precede.
        pin_level(&mut this, depth, &owner);
        // Children a filter reads (a pinned condition, a relation's input) loop first, in sibling
        // order, so each filter runs as soon as its captures are bound; the rest loop after.
        let mut read = Vec::new();
        level_pins(&this, &mut read);
        split.sort_by_key(|rel| {
            let mut patterns = Vec::new();
            rel.target.iter().for_each(|target| subtree_patterns(target, &mut patterns));
            !patterns.iter().any(|pattern| read.contains(pattern))
        });
        split.append(&mut this.rels);
        this.rels = split;
    }
    Ok(this)
}

/// Owner patterns of every pinned reference in `level`'s conditions and relations.
fn level_pins(level: &Level, out: &mut Vec<u16>) {
    fn cond_pins(cond: &Cond, out: &mut Vec<u16>) {
        match cond {
            Cond::TextEq(a, b) | Cond::Same(a, b) => out.extend(a.pin.iter().chain(b.pin.iter())),
            Cond::TextMatch(a, _) | Cond::Contains(a, _) => out.extend(a.pin),
            Cond::Not(inner) => cond_pins(inner, out),
            Cond::SiblingOrder(..) => {}
        }
    }
    level.conds.iter().for_each(|cond| cond_pins(cond, out));
    for rel in &level.rels {
        out.extend(rel.from.pin);
        rel.target.iter().chain(rel.stop.iter()).for_each(|inner| level_pins(inner, out));
    }
}

/// A split child's own pattern and the patterns of its own split children.
fn subtree_patterns(level: &Level, out: &mut Vec<u16>) {
    out.push(level.pattern);
    for rel in level.rels.iter().filter(|rel| rel.rows == Rows::Each && !rel.negated && !rel.optional) {
        rel.target.iter().for_each(|target| subtree_patterns(target, out));
    }
}

fn pin_ref(at: &mut CapRef, depth: u8, owner: &[(Box<str>, u16)]) {
    if at.level == depth && at.pin.is_none() {
        at.pin = owner.iter().find(|(name, _)| *name == at.name).map(|(_, pattern)| *pattern);
    }
}

fn pin_cond(cond: &mut Cond, depth: u8, owner: &[(Box<str>, u16)]) {
    match cond {
        Cond::TextEq(a, b) | Cond::Same(a, b) => {
            pin_ref(a, depth, owner);
            pin_ref(b, depth, owner);
        }
        Cond::TextMatch(a, _) | Cond::Contains(a, _) => pin_ref(a, depth, owner),
        Cond::Not(inner) => pin_cond(inner, depth, owner),
        Cond::SiblingOrder(..) => {}
    }
}

/// Stop and `of` levels open their own scope, so their references never point here.
fn pin_level(level: &mut Level, depth: u8, owner: &[(Box<str>, u16)]) {
    level.conds.iter_mut().for_each(|cond| pin_cond(cond, depth, owner));
    for rel in &mut level.rels {
        pin_ref(&mut rel.from, depth, owner);
        if let (Some(target), false) = (&mut rel.target, matches!(rel.walk, Walk::NthChild(_))) {
            pin_level(target, depth, owner);
        }
    }
}


/// A level root split into one level per child.
struct SplitPlan {
    /// Kept text predicates over the root's own captures, as flat-pattern text.
    root_preds: String,
    /// `(field, child text with the predicates it holds alone)`, in sibling order.
    children: Vec<(Option<String>, String)>,
    /// Two-capture `#eq?` / `#not-eq?` across children: `(negated, a, b)`.
    pinned: Vec<(bool, Box<str>, Box<str>)>,
}

/// A node somewhere inside `text` has 3+ unanchored sibling children.
fn holds_siblings(text: &str) -> bool {
    super::_2a_siblings::shape(text, 0).ok().flatten().is_some_and(|shape| {
        shape.unanchored() >= 3 || shape.children.iter().any(|child| holds_siblings(&child.text))
    })
}

/// Where each capture of a split level's subtree lives: its own pattern, then its split children's.
fn owners(level: &Level, patterns: &[FlatPattern], out: &mut Vec<(Box<str>, u16)>) {
    out.extend(patterns[level.pattern as usize].captures.iter().map(|name| (name.clone(), level.pattern)));
    for rel in level.rels.iter().filter(|rel| rel.rows == Rows::Each && !rel.negated && !rel.optional) {
        if let Some(target) = &rel.target {
            owners(target, patterns, out);
        }
    }
}

/// Splits a root with 3+ unanchored sibling children, or one holding such a node below it, into
/// one level per child; warns when a child cannot split.
fn split_plan(
    ctx: &mut Ctx,
    pattern: u16,
    shape: &super::_2a_siblings::Shape,
    kept: &[(&Pred, Vec<&str>)],
) -> Result<Option<SplitPlan>, ScmppError> {
    let siblings = shape.unanchored() >= 3;
    let nested = shape.children.iter().any(|child| holds_siblings(&child.text));
    if !siblings && !nested {
        return Ok(None);
    }
    let home = |names: &[&str]| {
        shape.children.iter().position(|child| {
            names.iter().all(|name| child.captures.iter().any(|seen| seen.as_ref() == *name))
        })
    };
    let at_root = |name: &str| !shape.children.iter().any(|child| child.captures.iter().any(|seen| seen.as_ref() == name));
    let mut plan = SplitPlan {
        root_preds: String::new(),
        children: shape.children.iter().map(|child| (child.field.clone(), child.text.clone())).collect(),
        pinned: Vec::new(),
    };
    let mut child_preds: Vec<Vec<&str>> = vec![Vec::new(); shape.children.len()];
    let mut placed = shape.splittable();
    for (pred, names) in kept {
        if names.iter().all(|name| at_root(name)) {
            plan.root_preds.push(' ');
            plan.root_preds.push_str(&pred.text);
        } else if let Some(index) = home(names) {
            child_preds[index].push(&pred.text);
        } else {
            match (pred.op.as_str(), pred.args.as_slice()) {
                (op @ ("eq?" | "not-eq?"), [Arg::Capture(a), Arg::Capture(b)]) => {
                    plan.pinned.push((op == "not-eq?", a.as_str().into(), b.as_str().into()))
                }
                _ => placed = false,
            }
        }
    }
    let shape_text = if siblings {
        format!("has {} unanchored sibling children", shape.unanchored())
    } else {
        "holds a node with 3+ unanchored sibling children".to_string()
    };
    ctx.warnings.push(format!(
        "scm++ level {pattern}: `({} ...)` {shape_text}; tree-sitter tries every ordered pick of them, so its cost grows combinatorially in the matching children{}",
        shape.kind,
        if placed { "; split into one parent edge per child" } else { "" },
    ));
    if !placed {
        return Ok(None);
    }
    for ((_, text), preds) in plan.children.iter_mut().zip(child_preds) {
        if !preds.is_empty() {
            *text = format!("({text} {})", preds.join(" "));
        }
    }
    Ok(Some(plan))
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
        optional: false,
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
            ("rows", Arg::Word(word)) if word == "list" => rel.rows = Rows::List,
            ("optional", Arg::Word(word)) if word == "true" || word == "false" => {
                rel.optional = word == "true"
            }
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
    if rel.optional && negated {
        return Err(unsupported(pred, "optional: does not apply to a not- relation"));
    }
    if rel.optional && rel.rows == Rows::List {
        return Err(unsupported(pred, "rows: list already keeps an outer match with no related node; drop optional:"));
    }
    let target = level(ctx, &text, base, inner)?;
    if rel.optional && target.rels.iter().any(|nested| !nested.negated && nested.rows != Rows::First) {
        return Err(unsupported(pred, "an optional: relation's target holds only rows: first relations"));
    }
    rel.target = Some(Box::new(target));
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
