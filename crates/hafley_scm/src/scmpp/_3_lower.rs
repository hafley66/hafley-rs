use super::_0_types::{CapRef, Cond, FlatPattern, Level, Rel, Rows, ScmppError, Walk};
use super::_2_compile::ROOT;

/// The store the SQL reads: every key an integer, every string behind a dictionary id.
pub const CAPTURE: &str = "scmpp_capture";
pub const NODE: &str = "scmpp_node";
pub const DICT_PATH: &str = "scmpp_dict_path";
pub const DICT_TEXT: &str = "scmpp_dict_text";

/// A level's captures that are not identity joins on an enclosing capture of the same name.
fn own_captures<'p>(level: &Level, patterns: &'p [FlatPattern]) -> Vec<&'p Box<str>> {
    patterns[level.pattern as usize]
        .captures
        .iter()
        .filter(|name| !level.conds.iter().any(|cond| matches!(cond, Cond::Same(inner, _) if inner.name == **name)))
        .collect()
}

/// The JSON array column of a `rows: list` relation: its target's first capture, `NAME__list`.
pub fn list_column(target: &Level, patterns: &[FlatPattern]) -> Option<Box<str>> {
    own_captures(target, patterns).first().map(|name| format!("{name}__list").into())
}

/// Exported captures in column order: level 0, then every `rows: each` level reached without `not-`.
/// A `rows: list` level exports one `NAME__list` column; its captures name the array's
/// object keys, a namespace of their own.
pub fn exports(plan: &Level, patterns: &[FlatPattern]) -> Result<Vec<(u16, Box<str>)>, ScmppError> {
    fn export_level(
        level: &Level,
        patterns: &[FlatPattern],
        out: &mut Vec<(u16, Box<str>)>,
    ) -> Result<(), ScmppError> {
        let push = |out: &mut Vec<(u16, Box<str>)>, name: &Box<str>| {
            if out.iter().any(|(_, seen)| seen == name) {
                return Err(ScmppError::Unsupported(format!(
                    "@{name} is bound by two levels; use two names and #eq?"
                )));
            }
            out.push((level.pattern, name.clone()));
            Ok(())
        };
        for name in own_captures(level, patterns) {
            push(out, name)?;
        }
        for rel in &level.rels {
            match (&rel.target, rel.rows, rel.negated) {
                (Some(target), Rows::Each, false) => export_level(target, patterns, out)?,
                (Some(target), Rows::List, false) => {
                    let column = list_column(target, patterns).ok_or_else(|| {
                        ScmppError::Unsupported("rows: list needs a @capture in its target".into())
                    })?;
                    push(out, &column)?;
                    export_level(target, patterns, &mut Vec::new())?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    export_level(plan, patterns, &mut out)?;
    Ok(out)
}

/// Dictionary entries the SQL names by id: capture names (`__root`, then every pattern's captures
/// in order) and relation fields in plan order. Entry `i` has id `i + 1`; the store seeds its
/// capture and field dictionaries with these lists before any other entry.
pub fn dictionaries(plan: &Level, patterns: &[FlatPattern]) -> (Vec<Box<str>>, Vec<Box<str>>) {
    fn fields(level: &Level, out: &mut Vec<Box<str>>) {
        for rel in &level.rels {
            if let Some(field) = &rel.field {
                if !out.contains(field) {
                    out.push(field.clone());
                }
            }
            for inner in rel.stop.iter().chain(rel.target.iter()) {
                fields(inner, out);
            }
        }
    }
    let mut captures: Vec<Box<str>> = vec![ROOT.into()];
    for name in patterns.iter().flat_map(|pattern| &pattern.captures) {
        if !captures.contains(name) {
            captures.push(name.clone());
        }
    }
    let mut out = Vec::new();
    fields(plan, &mut out);
    (captures, out)
}

fn lit(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn root(pattern: u16) -> String {
    format!("\"r{pattern}\"")
}

fn cap(pattern: u16, name: &str) -> String {
    format!("\"c{pattern}_{name}\"")
}

fn id_of(list: &[Box<str>], name: &str) -> usize {
    1 + list
        .iter()
        .position(|seen| seen.as_ref() == name)
        .expect("name in the plan's dictionary")
}

struct Join {
    left: bool,
    /// `CROSS JOIN`: SQLite keeps every table before it in an outer loop.
    cross: bool,
    table: String,
    alias: String,
    on: Vec<String>,
}

impl Join {
    fn node(alias: &str, on: Vec<String>) -> Self {
        Join {
            left: false,
            cross: false,
            table: NODE.into(),
            alias: alias.into(),
            on,
        }
    }
}

#[derive(Default)]
struct Body {
    from: Vec<Join>,
    wh: Vec<String>,
}

impl Body {
    fn render(self) -> (String, String) {
        let mut from = String::new();
        let mut wh = Vec::new();
        for (i, join) in self.from.into_iter().enumerate() {
            if i == 0 {
                from.push_str(&format!("{} AS {}", join.table, join.alias));
                wh.extend(join.on);
                continue;
            }
            let on = if join.on.is_empty() {
                "1 = 1".to_string()
            } else {
                join.on.join(" AND ")
            };
            let kind = match (join.left, join.cross) {
                (true, _) => "LEFT JOIN",
                (false, true) => "CROSS JOIN",
                (false, false) => "JOIN",
            };
            from.push_str(&format!(
                "\n  {kind} {} AS {} ON {on}",
                join.table, join.alias
            ));
        }
        wh.extend(self.wh);
        let wh = if wh.is_empty() {
            "1 = 1".to_string()
        } else {
            wh.join("\n  AND ")
        };
        (from, wh)
    }

    fn exists(self, negated: bool) -> String {
        let (from, wh) = self.render();
        let not = if negated { "NOT " } else { "" };
        format!("{not}EXISTS (SELECT 1 FROM {from}\n  WHERE {wh})")
    }
}

/// Columns a level's subtree reads off its captures, so only those joins are written.
fn referenced(level: &Level, depth: u8, out: &mut Vec<Box<str>>) {
    fn cond_refs<'c>(cond: &'c Cond, out: &mut Vec<&'c CapRef>) {
        match cond {
            Cond::TextEq(a, b) | Cond::Same(a, b) => out.extend([a, b]),
            Cond::TextMatch(a, _) | Cond::Contains(a, _) => out.push(a),
            Cond::Not(inner) => cond_refs(inner, out),
            Cond::SiblingOrder(..) => {}
        }
    }
    fn level_refs<'c>(level: &'c Level, out: &mut Vec<&'c CapRef>) {
        level.conds.iter().for_each(|cond| cond_refs(cond, out));
        for rel in &level.rels {
            out.push(&rel.from);
            if let (Some(target), false) = (&rel.target, matches!(rel.walk, Walk::NthChild(_))) {
                level_refs(target, out);
            }
        }
    }
    let mut refs = Vec::new();
    level_refs(level, &mut refs);
    for at in refs {
        // `@__root` is the level's own root row, joined already; a pinned capture joins at its owner.
        if at.level == depth && at.pin.is_none() && at.name.as_ref() != ROOT && !out.contains(&at.name) {
            out.push(at.name.clone());
        }
    }
}

/// One result column: `expr AS "key"`; `json` marks a JSON array text (`rows: list`).
struct Col {
    expr: String,
    key: String,
    json: bool,
}

struct Lower<'a> {
    patterns: &'a [FlatPattern],
    captures: &'a [Box<str>],
    fields: &'a [Box<str>],
    next: usize,
    select: Vec<Col>,
    /// Root aliases of the exported levels, outermost first: the row order.
    order: Vec<String>,
    /// Pinned captures (`CapRef.pin`), joined by whichever level owns their pattern.
    pinned: Vec<(u16, Box<str>)>,
    /// `WITH` entries the statement opens with.
    ctes: Vec<String>,
    /// `(child root alias, node-row alias)` of each one-step parent edge, for `sib` order.
    edges: Vec<(String, String)>,
    /// The next level root joins `CROSS`: split children loop in the order compile chose.
    cross_next: bool,
}

/// One statement over `scmpp_capture` / `scmpp_node`: integer joins and preorder ranges
/// (`pre`, `last`, `parent`, `sib`), no recursion; strings appear only through the dictionaries.
/// Also returns the result columns that hold a JSON array.
pub fn lower(
    plan: &Level,
    patterns: &[FlatPattern],
    captures: &[Box<str>],
    fields: &[Box<str>],
) -> (String, Vec<Box<str>>) {
    let mut lower = Lower {
        patterns,
        captures,
        fields,
        next: 0,
        select: Vec::new(),
        order: Vec::new(),
        pinned: Vec::new(),
        ctes: Vec::new(),
        edges: Vec::new(),
        cross_next: false,
    };
    let mut body = Body::default();
    lower.level(plan, &mut Vec::new(), &mut body, true);
    let (from, wh) = body.render();
    let order = std::iter::once("path".to_string())
        .chain(lower.order.iter().map(|r| format!("{r}.start, {r}.\"match\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let columns = std::iter::once(format!(
        "(SELECT p.text FROM {DICT_PATH} AS p WHERE p.id = {}.file) AS path",
        root(plan.pattern)
    ))
    .chain(lower.select.iter().map(|col| format!("{} AS \"{}\"", col.expr, col.key)))
    .collect::<Vec<_>>()
    .join(",\n  ");
    let lists = lower.select.iter().filter(|col| col.json).map(|col| col.key.as_str().into()).collect();
    let with = match lower.ctes.is_empty() {
        true => String::new(),
        false => format!("WITH {}\n", lower.ctes.join(",\n")),
    };
    (format!("{with}SELECT {columns}\nFROM {from}\nWHERE {wh}\nORDER BY {order}"), lists)
}

impl Lower<'_> {
    fn alias(&self, at: &CapRef, chain: &[u16]) -> String {
        let pattern = at.pin.unwrap_or_else(|| chain[at.level as usize]);
        match at.name.as_ref() {
            ROOT => root(pattern),
            name => cap(pattern, name),
        }
    }

    /// Joins the level's root and captures into `body`; `export` adds its columns to the SELECT.
    fn level(&mut self, level: &Level, chain: &mut Vec<u16>, body: &mut Body, export: bool) {
        let pattern = level.pattern;
        let r = root(pattern);
        let mut on = vec![
            format!("{r}.pattern = {pattern}"),
            format!("{r}.capture = {}", id_of(self.captures, ROOT)),
        ];
        if let Some(parent) = chain.last() {
            on.push(format!("{r}.file = {}.file", root(*parent)));
        }
        body.from.push(Join {
            left: false,
            cross: std::mem::take(&mut self.cross_next),
            table: CAPTURE.into(),
            alias: r.clone(),
            on,
        });
        if export {
            self.order.push(r.clone());
        }
        let depth = chain.len() as u8;
        chain.push(pattern);
        let mut names = Vec::new();
        if export {
            names.extend(self.patterns[pattern as usize].captures.iter().cloned());
        }
        referenced(level, depth, &mut names);
        fn pin(at: &CapRef, out: &mut Vec<(u16, Box<str>)>) {
            if let Some(owner) = at.pin {
                out.push((owner, at.name.clone()));
            }
        }
        fn cond_pins(cond: &Cond, out: &mut Vec<(u16, Box<str>)>) {
            match cond {
                Cond::TextEq(a, b) | Cond::Same(a, b) => {
                    pin(a, out);
                    pin(b, out);
                }
                Cond::TextMatch(a, _) | Cond::Contains(a, _) => pin(a, out),
                Cond::Not(inner) => cond_pins(inner, out),
                Cond::SiblingOrder(..) => {}
            }
        }
        fn level_pins(level: &Level, out: &mut Vec<(u16, Box<str>)>) {
            level.conds.iter().for_each(|cond| cond_pins(cond, out));
            for rel in &level.rels {
                pin(&rel.from, out);
                rel.target.iter().chain(rel.stop.iter()).for_each(|inner| level_pins(inner, out));
            }
        }
        level_pins(level, &mut self.pinned);
        for (owner, name) in &self.pinned {
            if *owner == pattern && !names.contains(name) {
                names.push(name.clone());
            }
        }
        for name in &names {
            let joined = level.conds.iter().any(|cond| matches!(cond, Cond::Same(inner, _) if inner.level == depth && inner.name == *name));
            self.capture(pattern, name, body, export && !joined, false);
        }
        // Relations first: a sibling order reads the parent-edge rows their joins open.
        for rel in &level.rels {
            self.rel(rel, chain, body, export);
        }
        for cond in &level.conds {
            body.wh.push(self.cond(cond, chain));
        }
        chain.pop();
    }

    /// Left-joins one capture of the level's match; `select` adds its columns.
    /// `absent`: the level itself may be missing (an `optional:` target), so every capture joins LEFT.
    fn capture(&mut self, pattern: u16, name: &str, body: &mut Body, select: bool, absent: bool) {
        let (r, alias) = (root(pattern), cap(pattern, name));
        if select {
            let col = |expr: String, suffix: &str| Col {
                expr,
                key: format!("{name}__{suffix}"),
                json: false,
            };
            self.select.push(col(format!("{alias}.\"start\""), "start"));
            self.select.push(col(format!("{alias}.\"end\""), "end"));
            self.select.push(col(
                format!("(SELECT t.text FROM {DICT_TEXT} AS t WHERE t.id = {alias}.text)"),
                "text",
            ));
        }
        // A capture every match carries joins inner, so the planner may start from it.
        let left = absent || self.patterns[pattern as usize].optional.iter().any(|seen| seen.as_ref() == name);
        body.from.push(Join {
            left,
            cross: false,
            table: CAPTURE.into(),
            on: vec![
                format!("{alias}.file = {r}.file"),
                format!("{alias}.pattern = {pattern}"),
                format!("{alias}.\"match\" = {r}.\"match\""),
                format!("{alias}.capture = {}", id_of(self.captures, name)),
            ],
            alias,
        });
    }

    fn cond(&self, cond: &Cond, chain: &[u16]) -> String {
        // A text predicate runs once per dictionary entry; the capture's text id is tested against that set.
        let texts = |a: &CapRef, test: String| {
            format!(
                "{}.text IN (SELECT t.id FROM {DICT_TEXT} AS t WHERE {test})",
                self.alias(a, chain)
            )
        };
        match cond {
            Cond::TextEq(a, b) => format!(
                "{}.text = {}.text",
                self.alias(a, chain),
                self.alias(b, chain)
            ),
            Cond::TextMatch(a, pattern) => texts(a, format!("regexp({}, t.text)", lit(pattern))),
            Cond::Contains(a, literals) => texts(
                a,
                literals
                    .iter()
                    .map(|literal| format!("instr(t.text, {}) > 0", lit(literal)))
                    .collect::<Vec<_>>()
                    .join(" AND "),
            ),
            Cond::Same(a, b) => {
                let (a, b) = (self.alias(a, chain), self.alias(b, chain));
                format!("{a}.file = {b}.file AND {a}.node = {b}.node")
            }
            // The split children's parent-edge rows carry `sib`; `(file, parent, sib)` is indexed,
            // so the order is a range the planner can drive a loop with.
            Cond::SiblingOrder(before, after) => {
                let sib = |pattern: u16| {
                    let r = root(pattern);
                    match self.edges.iter().find(|(child, _)| *child == r) {
                        Some((_, edge)) => format!("{edge}.sib"),
                        None => format!("(SELECT n.sib FROM {NODE} AS n WHERE n.file = {r}.file AND n.pre = {r}.node)"),
                    }
                };
                format!("{} < {}", sib(*before), sib(*after))
            }
            // An absent optional capture makes the inner test NULL; the `not-` form holds then.
            Cond::Not(inner) => format!("NOT COALESCE(({}), 0)", self.cond(inner, chain)),
        }
    }

    /// `EXISTS` of a scope-root level (stop, `of`); `tie` relates its root capture to the outer rows.
    fn scope(
        &mut self,
        level: &Level,
        negated: bool,
        tie: impl FnOnce(&str) -> (Vec<Join>, Vec<String>),
    ) -> String {
        let mut body = Body::default();
        self.level(level, &mut Vec::new(), &mut body, false);
        let (joins, wh) = tie(&root(level.pattern));
        body.from.extend(joins);
        body.wh.extend(wh);
        body.exists(negated)
    }

    /// The scope-root level matches at the node row `node`.
    fn node_is(&mut self, level: &Level, node: &str) -> String {
        self.scope(level, false, |r| {
            (
                Vec::new(),
                vec![format!("{r}.file = {node}.file AND {r}.node = {node}.pre")],
            )
        })
    }

    fn rel(&mut self, rel: &Rel, chain: &mut Vec<u16>, body: &mut Body, export: bool) {
        let from = self.alias(&rel.from, chain);
        if let Walk::NthChild(index) = rel.walk {
            let n = self.fresh();
            let me = format!("\"m{n}\"");
            let found = match rel.target.as_deref() {
                // A named child's `sib` is its index among the named children: one row, no count.
                None => format!(
                    "EXISTS (SELECT 1 FROM {NODE} AS {me}\n  WHERE {me}.file = {from}.file AND {me}.pre = {from}.node AND {me}.sib = {})",
                    index - 1
                ),
                // Rank among the siblings that match `of`, computed once for the statement.
                Some(of) => {
                    let (ranked, node) = (format!("nth{n}"), format!("\"q{n}\""));
                    let matches = self.node_is(of, &node);
                    self.ctes.push(format!(
                        "{ranked} AS MATERIALIZED (SELECT {node}.file AS file, {node}.pre AS pre, \
                         row_number() OVER (PARTITION BY {node}.file, {node}.parent ORDER BY {node}.sib) AS rank\n  \
                         FROM {NODE} AS {node} WHERE {node}.sib IS NOT NULL AND {matches})"
                    ));
                    format!("EXISTS (SELECT 1 FROM {ranked} WHERE {ranked}.file = {from}.file AND {ranked}.pre = {from}.node AND {ranked}.rank = {index})")
                }
            };
            body.wh.push(if rel.negated {
                format!("NOT {found}")
            } else {
                found
            });
            return;
        }
        let target = rel.target.as_deref().expect("relation target");
        if rel.optional {
            // Holds with or without a related node; exported `rows: each` captures left-join.
            if rel.rows == Rows::Each && export {
                self.optional(rel, target, &from, chain, body);
            }
            return;
        }
        if rel.rows == Rows::List && !rel.negated {
            // Zero related nodes is `[]`, so the relation never drops the outer match.
            if export {
                self.list(rel, target, &from, chain);
            }
            return;
        }
        let each = rel.rows == Rows::Each && !rel.negated;
        let mut inner = Body::default();
        let sink = if each { &mut *body } else { &mut inner };
        // A split child (the only relation from `@__root`) loops where compile ordered it.
        self.cross_next = each && rel.neighbor && rel.from.name.as_ref() == ROOT;
        self.level(target, chain, sink, export && each);
        self.step(rel, &from, &root(target.pattern), sink);
        if !each {
            body.wh.push(inner.exists(rel.negated));
        }
    }

    /// `rows: list`: one correlated aggregate per outer row, an array of the target level's
    /// captures (and its `rows: each` / `rows: list` levels), one object per row, in `pre` order.
    fn list(&mut self, rel: &Rel, target: &Level, from: &str, chain: &mut Vec<u16>) {
        let select = std::mem::take(&mut self.select);
        let order = std::mem::take(&mut self.order);
        let mut inner = Body::default();
        self.level(target, chain, &mut inner, true);
        self.step(rel, from, &root(target.pattern), &mut inner);
        let cols = std::mem::replace(&mut self.select, select);
        let roots = std::mem::replace(&mut self.order, order);
        let pairs = cols
            .iter()
            .map(|col| match col.json {
                true => format!("{}, json({})", lit(&col.key), col.expr),
                false => format!("{}, {}", lit(&col.key), col.expr),
            })
            .collect::<Vec<_>>()
            .join(",\n    ");
        let by = roots
            .iter()
            .map(|r| format!("{r}.node, {r}.\"match\""))
            .collect::<Vec<_>>()
            .join(", ");
        let (body, wh) = inner.render();
        self.select.push(Col {
            expr: format!(
                "(SELECT json_group_array(json_object(\n    {pairs}) ORDER BY {by})\n  FROM {body}\n  WHERE {wh})"
            ),
            key: list_column(target, self.patterns).expect("exports checked the list column").into(),
            json: true,
        });
    }

    /// `rows: each` with `optional: true`: the target root left-joins on the match numbers that
    /// relate to `from` and pass the level's predicates, so an outer row with none keeps nulls.
    fn optional(&mut self, rel: &Rel, target: &Level, from: &str, chain: &mut Vec<u16>, body: &mut Body) {
        let (pattern, r) = (target.pattern, root(target.pattern));
        let mut inner = Body::default();
        self.level(target, chain, &mut inner, false);
        self.step(rel, from, &r, &mut inner);
        let (matched, wh) = inner.render();
        let parent = root(*chain.last().expect("a relation sits inside a level"));
        body.from.push(Join {
            left: true,
            cross: false,
            table: CAPTURE.into(),
            alias: r.clone(),
            on: vec![
                format!("{r}.pattern = {pattern}"),
                format!("{r}.capture = {}", id_of(self.captures, ROOT)),
                format!("{r}.file = {parent}.file"),
                format!("{r}.\"match\" IN (SELECT {r}.\"match\" FROM {matched}\n  WHERE {wh})"),
            ],
        });
        self.order.push(r);
        for name in own_captures(target, self.patterns) {
            self.capture(pattern, name, body, true, true);
        }
    }

    fn fresh(&mut self) -> usize {
        self.next += 1;
        self.next - 1
    }

    /// The node rows that relate the capture `from` to the target root capture `to`.
    fn step(&mut self, rel: &Rel, from: &str, to: &str, body: &mut Body) {
        let n = self.fresh();
        let field = rel
            .field
            .as_ref()
            .map(|name| id_of(self.fields, name));
        let field_is = |alias: &str| field.map(|id| format!("{alias}.field = {id}"));
        match (rel.walk, rel.neighbor) {
            (Walk::Parent, _) | (Walk::Ancestor, true) | (Walk::Descendant, true) => {
                let (child, parent) = if rel.walk == Walk::Descendant {
                    (to, from)
                } else {
                    (from, to)
                };
                let edge = format!("\"e{n}\"");
                let mut on = vec![
                    format!("{edge}.file = {child}.file"),
                    format!("{edge}.pre = {child}.node"),
                    format!("{edge}.parent = {parent}.node"),
                ];
                on.extend(field_is(&edge));
                body.from.push(Join::node(&edge, on));
                self.edges.push((child.to_string(), edge));
            }
            (Walk::Ancestor, false) => {
                // The target encloses `from`; a stop node strictly between them ends the walk.
                let above = format!("\"a{n}\"");
                let mut on = vec![
                    format!("{above}.file = {to}.file"),
                    format!("{above}.pre = {to}.node"),
                    // The range sits on the target capture's `node`, so its index drives the search.
                    format!("{to}.node < {from}.node"),
                    format!("{from}.node <= {above}.last"),
                ];
                if let Some(stop) = rel.stop.as_deref() {
                    let between = format!("\"b{n}\"");
                    on.push(self.scope(stop, true, |r| {
                        (
                            vec![Join::node(
                                &between,
                                vec![
                                    format!("{between}.file = {r}.file"),
                                    format!("{between}.pre = {r}.node"),
                                    format!("{between}.last >= {from}.node"),
                                ],
                            )],
                            vec![
                                format!("{r}.file = {from}.file"),
                                format!("{r}.node > {above}.pre"),
                                format!("{r}.node < {from}.node"),
                            ],
                        )
                    }));
                }
                body.from.push(Join::node(&above, on));
                if let Some(id) = field {
                    // The field belongs to the target's child on the path down to `from`.
                    let child = format!("\"k{n}\"");
                    body.from.push(Join::node(
                        &child,
                        vec![
                            format!("{child}.file = {above}.file"),
                            format!("{child}.parent = {above}.pre"),
                            format!("{child}.pre <= {from}.node"),
                            format!("{from}.node <= {child}.last"),
                            format!("{child}.field = {id}"),
                        ],
                    ));
                }
            }
            (Walk::Descendant, false) => {
                // `from` encloses the target; a stop node strictly between them ends the walk.
                let below = format!("\"d{n}\"");
                let mut on = vec![
                    format!("{below}.file = {from}.file"),
                    format!("{below}.pre = {from}.node"),
                    format!("{to}.node > {below}.pre"),
                    format!("{to}.node <= {below}.last"),
                ];
                if let Some(stop) = rel.stop.as_deref() {
                    let between = format!("\"b{n}\"");
                    on.push(self.scope(stop, true, |r| {
                        (
                            vec![Join::node(
                                &between,
                                vec![
                                    format!("{between}.file = {r}.file"),
                                    format!("{between}.pre = {r}.node"),
                                    format!("{between}.last >= {to}.node"),
                                ],
                            )],
                            vec![
                                format!("{r}.file = {from}.file"),
                                format!("{r}.node > {below}.pre"),
                                format!("{r}.node < {to}.node"),
                            ],
                        )
                    }));
                }
                body.from.push(Join::node(&below, on));
                if let Some(id) = field {
                    let reached = format!("\"k{n}\"");
                    body.from.push(Join::node(
                        &reached,
                        vec![
                            format!("{reached}.file = {to}.file"),
                            format!("{reached}.pre = {to}.node"),
                            format!("{reached}.field = {id}"),
                        ],
                    ));
                }
            }
            (Walk::Precedes | Walk::Follows, _) => {
                let (mine, theirs) = (format!("\"e{n}a\""), format!("\"e{n}b\""));
                let order = match (rel.walk, rel.neighbor) {
                    (Walk::Precedes, true) => format!("{theirs}.sib = {mine}.sib + 1"),
                    (Walk::Precedes, false) => format!("{theirs}.sib > {mine}.sib"),
                    (_, true) => format!("{theirs}.sib = {mine}.sib - 1"),
                    (_, false) => format!("{theirs}.sib < {mine}.sib"),
                };
                body.from.push(Join::node(
                    &mine,
                    vec![
                        format!("{mine}.file = {from}.file"),
                        format!("{mine}.pre = {from}.node"),
                    ],
                ));
                let mut on = vec![
                    format!("{theirs}.file = {to}.file"),
                    format!("{theirs}.pre = {to}.node"),
                    format!("{theirs}.parent = {mine}.parent"),
                    order,
                ];
                on.extend(field_is(&theirs));
                if let Some(stop) = rel.stop.as_deref() {
                    let between = format!("\"e{n}c\"");
                    let (low, high) = if rel.walk == Walk::Precedes {
                        (&mine, &theirs)
                    } else {
                        (&theirs, &mine)
                    };
                    on.push(self.scope(stop, true, |r| {
                        (
                            vec![Join::node(
                                &between,
                                vec![
                                    format!("{between}.file = {r}.file"),
                                    format!("{between}.pre = {r}.node"),
                                ],
                            )],
                            vec![
                                format!("{r}.file = {mine}.file"),
                                format!("{between}.parent = {mine}.parent"),
                                format!("{between}.sib > {low}.sib"),
                                format!("{between}.sib < {high}.sib"),
                            ],
                        )
                    }));
                }
                body.from.push(Join::node(&theirs, on));
            }
            (Walk::NthChild(_), _) => unreachable!("nth-child lowers in rel"),
        }
    }
}
