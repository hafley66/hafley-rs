use super::_0_types::{CapRef, Cond, FlatPattern, Level, Rel, Rows, ScmppError, Walk};

/// Exported captures in column order: level 0, then every `rows: each` level reached without `not-`.
pub fn exports(plan: &Level, patterns: &[FlatPattern]) -> Result<Vec<(u16, Box<str>)>, ScmppError> {
    fn export_level(
        level: &Level,
        patterns: &[FlatPattern],
        depth: u8,
        out: &mut Vec<(u16, Box<str>)>,
    ) -> Result<(), ScmppError> {
        for name in &patterns[level.pattern as usize].captures {
            let joined = level.conds.iter().any(|cond| matches!(cond, Cond::Same(inner, _) if inner.level == depth && inner.name == *name));
            if joined {
                continue;
            }
            if out.iter().any(|(_, seen)| seen == name) {
                return Err(ScmppError::Unsupported(format!(
                    "@{name} is bound by two levels; use two names and #eq?"
                )));
            }
            out.push((level.pattern, name.clone()));
        }
        for rel in &level.rels {
            if let (Some(target), Rows::Each, false) = (&rel.target, rel.rows, rel.negated) {
                export_level(target, patterns, depth + 1, out)?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    export_level(plan, patterns, 0, &mut out)?;
    Ok(out)
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

/// Node key columns of a row source, in (content, start, end, kind) order.
struct Key {
    cid: String,
    start: String,
    end: String,
    kind: String,
}

impl Key {
    fn of(alias: &str) -> Self {
        Key {
            cid: format!("{alias}._content_id"),
            start: format!("{alias}.start"),
            end: format!("{alias}.\"end\""),
            kind: format!("{alias}.kind"),
        }
    }

    fn edge(alias: &str, side: &str) -> Self {
        Key {
            cid: format!("{alias}._content_id"),
            start: format!("{alias}.{side}__start"),
            end: format!("{alias}.{side}__end"),
            kind: format!("{alias}.{side}_kind"),
        }
    }

    fn walk(alias: &str) -> Self {
        Key {
            cid: format!("{alias}.cid"),
            start: format!("{alias}.start"),
            end: format!("{alias}.\"end\""),
            kind: format!("{alias}.kind"),
        }
    }

    fn same_node(&self, other: &Key) -> String {
        format!(
            "{} = {} AND {} = {} AND {} = {}",
            self.start, other.start, self.end, other.end, self.kind, other.kind
        )
    }

    fn same(&self, other: &Key) -> String {
        format!("{} = {} AND {}", self.cid, other.cid, self.same_node(other))
    }
}

struct Join {
    left: bool,
    table: String,
    alias: String,
    on: Vec<String>,
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
                "1".to_string()
            } else {
                join.on.join(" AND ")
            };
            let kind = if join.left { "LEFT JOIN" } else { "JOIN" };
            from.push_str(&format!(
                "\n  {kind} {} AS {} ON {on}",
                join.table, join.alias
            ));
        }
        wh.extend(self.wh);
        let wh = if wh.is_empty() {
            "1".to_string()
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
        if at.level == depth && !out.contains(&at.name) {
            out.push(at.name.clone());
        }
    }
}

struct Lower<'a> {
    patterns: &'a [FlatPattern],
    ctes: Vec<String>,
    next: usize,
    select: Vec<String>,
    order: Vec<String>,
}

pub fn lower(plan: &Level, patterns: &[FlatPattern]) -> String {
    let mut lower = Lower {
        patterns,
        ctes: Vec::new(),
        next: 0,
        select: vec![format!("{}._input_path AS path", root(plan.pattern))],
        order: Vec::new(),
    };
    let mut body = Body::default();
    lower.level(plan, &mut Vec::new(), &mut body, true);
    let (from, wh) = body.render();
    let with = if lower.ctes.is_empty() {
        String::new()
    } else {
        format!("WITH RECURSIVE\n{}\n", lower.ctes.join(",\n"))
    };
    let order = std::iter::once(format!("{}._input_path", root(plan.pattern)))
        .chain(lower.order)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{with}SELECT {}\nFROM {from}\nWHERE {wh}\nORDER BY {order}",
        lower.select.join(",\n  "),
    )
}

impl Lower<'_> {
    fn alias(&self, at: &CapRef, chain: &[u16]) -> String {
        cap(chain[at.level as usize], &at.name)
    }

    /// Joins the level's root and captures into `body`; `export` adds its columns to the SELECT.
    fn level(&mut self, level: &Level, chain: &mut Vec<u16>, body: &mut Body, export: bool) {
        let pattern = level.pattern;
        let r = root(pattern);
        let mut on = vec![
            format!("{r}.pattern = {pattern}"),
            format!("{r}.capture = '__root'"),
        ];
        if let Some(parent) = chain.last() {
            on.push(format!("{r}._input_path = {}._input_path", root(*parent)));
        }
        body.from.push(Join {
            left: false,
            table: "capture".into(),
            alias: r.clone(),
            on,
        });
        if export {
            self.order.push(format!("{r}.start"));
            self.order.push(format!("{r}.\"match\""));
        }
        let depth = chain.len() as u8;
        chain.push(pattern);
        let mut names = Vec::new();
        if export {
            names.extend(self.patterns[pattern as usize].captures.iter().cloned());
        }
        referenced(level, depth, &mut names);
        for name in &names {
            let alias = cap(pattern, name);
            let joined = level.conds.iter().any(|cond| matches!(cond, Cond::Same(inner, _) if inner.level == depth && inner.name == *name));
            if export && !joined {
                for column in ["start", "end", "text"] {
                    self.select
                        .push(format!("{alias}.\"{column}\" AS \"{name}__{column}\""));
                }
            }
            body.from.push(Join {
                left: true,
                table: "capture".into(),
                alias,
                on: vec![
                    format!("{}._input_path = {r}._input_path", cap(pattern, name)),
                    format!("{}.pattern = {pattern}", cap(pattern, name)),
                    format!("{}.\"match\" = {r}.\"match\"", cap(pattern, name)),
                    format!("{}.capture = {}", cap(pattern, name), lit(name)),
                ],
            });
        }
        for cond in &level.conds {
            body.wh.push(self.cond(cond, chain));
        }
        for rel in &level.rels {
            self.rel(rel, chain, body, export);
        }
        chain.pop();
    }

    fn cond(&self, cond: &Cond, chain: &[u16]) -> String {
        match cond {
            Cond::TextEq(a, b) => format!(
                "{}.text = {}.text",
                self.alias(a, chain),
                self.alias(b, chain)
            ),
            Cond::TextMatch(a, pattern) => {
                format!("regexp({}, {}.text)", lit(pattern), self.alias(a, chain))
            }
            Cond::Contains(a, literals) => literals
                .iter()
                .map(|literal| {
                    format!("instr({}.text, {}) > 0", self.alias(a, chain), lit(literal))
                })
                .collect::<Vec<_>>()
                .join(" AND "),
            Cond::Same(a, b) => {
                Key::of(&self.alias(a, chain)).same(&Key::of(&self.alias(b, chain)))
            }
            Cond::Not(inner) => format!("NOT ({})", self.cond(inner, chain)),
        }
    }

    /// `EXISTS` of a scope-root level (stop, `of`) whose root is the node at `key`.
    fn node_matches(&mut self, level: &Level, key: &Key) -> String {
        let mut body = Body::default();
        self.level(level, &mut Vec::new(), &mut body, false);
        body.wh.push(Key::of(&root(level.pattern)).same(key));
        body.exists(false)
    }

    fn rel(&mut self, rel: &Rel, chain: &mut Vec<u16>, body: &mut Body, export: bool) {
        let from = Key::of(&self.alias(&rel.from, chain));
        if let Walk::NthChild(index) = rel.walk {
            let cte = self.nth_cte(rel.target.as_deref());
            let found = format!(
                "EXISTS (SELECT 1 FROM {cte} AS s WHERE {} AND s.n = {index})",
                Key::walk("s").same(&from)
            );
            body.wh.push(if rel.negated {
                format!("NOT {found}")
            } else {
                found
            });
            return;
        }
        let seed = chain[rel.from.level as usize];
        let target = rel.target.as_deref().expect("relation target");
        let each = rel.rows == Rows::Each && !rel.negated;
        let mut inner = Body::default();
        let sink = if each { &mut *body } else { &mut inner };
        self.level(target, chain, sink, export && each);
        self.step(rel, seed, &from, &Key::of(&root(target.pattern)), sink);
        if !each {
            body.wh.push(inner.exists(rel.negated));
        }
    }

    fn fresh(&mut self) -> usize {
        self.next += 1;
        self.next - 1
    }

    /// The edge or walk rows that relate `from` (a capture of pattern `seed`) to the target root `to`.
    fn step(&mut self, rel: &Rel, seed: u16, from: &Key, to: &Key, body: &mut Body) {
        let n = self.fresh();
        let field = |alias: &str| {
            rel.field
                .as_ref()
                .map(|name| format!("{alias}.field = {}", lit(name)))
        };
        let into = |alias: &str, child: &Key| {
            vec![
                format!("{alias}.family = 'cst'"),
                Key::edge(alias, "to").same(child),
            ]
        };
        match (rel.walk, rel.neighbor) {
            (Walk::Parent, _) | (Walk::Ancestor, true) | (Walk::Descendant, true) => {
                let alias = format!("\"e{n}\"");
                let (child, parent) = if rel.walk == Walk::Descendant {
                    (to, from)
                } else {
                    (from, to)
                };
                let mut on = into(&alias, child);
                on.push(Key::edge(&alias, "from").same(parent));
                on.extend(field(&alias));
                body.from.push(Join {
                    left: false,
                    table: "edge".into(),
                    alias,
                    on,
                });
            }
            (Walk::Ancestor | Walk::Descendant, false) => {
                let cte = self.walk_cte(rel, seed);
                let alias = format!("\"w{n}\"");
                let mut on = vec![
                    format!("{alias}.cid = {}", from.cid),
                    format!(
                        "{alias}.fs = {} AND {alias}.fe = {} AND {alias}.fk = {}",
                        from.start, from.end, from.kind
                    ),
                    // The content id lets the target's capture index seek, not scan every root.
                    Key::walk(&alias).same(to),
                ];
                on.extend(field(&alias));
                body.from.push(Join {
                    left: false,
                    table: cte,
                    alias,
                    on,
                });
            }
            (Walk::Precedes | Walk::Follows, _) => {
                let (mine, theirs) = (format!("\"e{n}a\""), format!("\"e{n}b\""));
                let order = match (rel.walk, rel.neighbor) {
                    (Walk::Precedes, true) => {
                        format!("{theirs}.named_index = {mine}.named_index + 1")
                    }
                    (Walk::Precedes, false) => format!("{theirs}.named_index > {mine}.named_index"),
                    (_, true) => format!("{theirs}.named_index = {mine}.named_index - 1"),
                    (_, false) => format!("{theirs}.named_index < {mine}.named_index"),
                };
                let siblings =
                    |alias: &str| Key::edge(alias, "from").same(&Key::edge(&mine, "from"));
                body.from.push(Join {
                    left: false,
                    table: "edge".into(),
                    alias: mine.clone(),
                    on: into(&mine, from),
                });
                let mut on = into(&theirs, to);
                on.push(siblings(&theirs));
                on.push(order);
                on.extend(field(&theirs));
                if let Some(stop) = rel.stop.as_deref() {
                    let between = format!("\"e{n}c\"");
                    let blocked = self.node_matches(stop, &Key::edge(&between, "to"));
                    on.push(format!(
                        "NOT EXISTS (SELECT 1 FROM edge AS {between} WHERE {between}.family = 'cst' AND {}\n  \
                         AND {between}.named_index > min({mine}.named_index, {theirs}.named_index)\n  \
                         AND {between}.named_index < max({mine}.named_index, {theirs}.named_index)\n  AND {blocked})",
                        siblings(&between)
                    ));
                }
                body.from.push(Join {
                    left: false,
                    table: "edge".into(),
                    alias: theirs,
                    on,
                });
            }
            (Walk::NthChild(_), _) => unreachable!("nth-child lowers in rel"),
        }
    }

    /// Closure from every node the `from` capture binds, down or up `edge`; `stopBy` ends a branch inclusively.
    fn walk_cte(&mut self, rel: &Rel, seed: u16) -> String {
        let name = format!("walk{}", self.fresh());
        let (near, far) = if rel.walk == Walk::Descendant {
            ("from", "to")
        } else {
            ("to", "from")
        };
        let stop = rel.stop.as_deref().map(|stop| {
            format!(
                "\n    WHERE NOT {}",
                self.node_matches(stop, &Key::walk("w"))
            )
        });
        let step = |alias: &str| {
            format!(
                "JOIN edge AS e ON e._content_id = {alias}.cid AND e.family = 'cst' AND e.{near}__start = {alias}.start \
                 AND e.{near}__end = {alias}.\"end\" AND e.{near}_kind = {alias}.kind"
            )
        };
        self.ctes.push(format!(
            "{name}(cid, fs, fe, fk, start, \"end\", kind, field) AS (\n    \
             SELECT s.cid, s.start, s.\"end\", s.kind, e.{far}__start, e.{far}__end, e.{far}_kind, e.field\n    \
             FROM (SELECT DISTINCT _content_id AS cid, start, \"end\", kind FROM capture WHERE pattern = {seed} AND capture = {}) AS s\n    {}\n    \
             UNION\n    \
             SELECT w.cid, w.fs, w.fe, w.fk, e.{far}__start, e.{far}__end, e.{far}_kind, e.field\n    \
             FROM {name} AS w {}{})",
            lit(&rel.from.name),
            step("s"),
            step("w"),
            stop.unwrap_or_default(),
        ));
        name
    }

    /// 1-based position of each named child among its siblings that match `of`.
    fn nth_cte(&mut self, of: Option<&Level>) -> String {
        let name = format!("nth{}", self.fresh());
        let filter = of
            .map(|level| {
                format!(
                    "\n    AND {}",
                    self.node_matches(level, &Key::edge("e", "to"))
                )
            })
            .unwrap_or_default();
        self.ctes.push(format!(
            "{name}(cid, start, \"end\", kind, n) AS (\n    \
             SELECT e._content_id, e.to__start, e.to__end, e.to_kind, ROW_NUMBER() OVER (\
             PARTITION BY e._content_id, e.from__start, e.from__end, e.from_kind ORDER BY e.named_index)\n    \
             FROM edge AS e WHERE e.family = 'cst' AND e.named_index IS NOT NULL{filter})"
        ));
        name
    }
}
