//! `.scm` surface syntax lowered into [`AstRule`]. The query text is parsed by
//! tree-sitter-tsquery (the grammar for the query language itself), walked, and
//! rewritten into the rule model `ast_grep_core` evaluates. No matching operator
//! is implemented here: containment, sibling order, and regex all stay with
//! ast-grep.
//! @comment-ok: module header, the shape every lang/*.rs opens with
//!
//! # Plan
//!
//! ```text
//! pub struct ScmProgram { rule: AstRule, utils: Vec<NamedAstRule> }
//! pub enum ScmLowerError { Parse, Syntax, UnknownPredicate, PredicateArity,
//!                          UnboundReference, DuplicateLabel, FocusConflict }
//! pub fn lower_scm(text: &str) -> Result<ScmProgram, ScmLowerError>
//! ```
//!
//! Body, in order:
//!
//! 1. Parse the text. A grammar that returns no tree is `Parse`.
//! 2. Reject the tree when it carries an ERROR or MISSING node: `Syntax` with
//!    the node's zero-based row.
//! 3. Collect the `program`'s top-level `definition` children. A definition
//!    carrying a direct `capture` child is labelled; its label is a `utils`
//!    entry id and a second use of one label is `DuplicateLabel`.
//! 4. Lower every labelled definition into `utils`, ordered by appearance.
//! 5. Lower the unlabelled definitions into `rule`: one definition directly,
//!    several under `Any`, none as `Any` over every `utils` id.
//! 6. Inside one definition, dispatch each `predicate` by name. Every predicate
//!    names the capture it constrains; that capture's host node is the rule's
//!    root, because an ast-grep rule reports exactly one node per match.
//!
//! # What the surface carries that `AstRule` cannot
//!
//! Field selectors (`function:`), supertypes (`expression/identifier`), and
//! quantifiers (`*`, `+`, `?`) have no operator in the rule model, so the walk
//! drops them and keeps the node constraint underneath. A `negated_field`
//! lowers its field name as a kind, which holds only where a grammar spells a
//! field and a node type the same way. The wildcard `(_)` and `(MISSING x)` are
//! `Syntax`.

use std::collections::BTreeSet;

use tree_sitter::{Node, Parser, Tree};

use crate::lang::ast_rule::{AstRule, NamedAstRule, StopBy};

/// The rule the file's unlabelled patterns ask for, plus every labelled pattern
/// under its capture label. `utils` is appearance-ordered with unique ids.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScmProgram {
    pub rule: AstRule,
    pub utils: Vec<NamedAstRule>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScmLowerError {
    /// The grammar produced no tree for the text.
    Parse(String),
    /// An ERROR or MISSING node stands in the query CST at zero-based `row`.
    Syntax { row: u32, message: String },
    /// A predicate spelled `#foo?` with no `AstRule` mapping.
    UnknownPredicate(String),
    /// A known predicate given a parameter count it does not take.
    PredicateArity { operator: String, got: usize },
    /// An identifier argument naming no top-level label, or a predicate naming
    /// a capture that its own pattern never binds.
    UnboundReference(String),
    /// One capture label used by two top-level definitions.
    DuplicateLabel(String),
    /// Two predicates in one definition constraining two different captures.
    /// An ast-grep rule reports one node, so one definition has one root.
    FocusConflict { first: String, second: String },
}

impl std::fmt::Display for ScmLowerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for ScmLowerError {}

/// Parse `.scm` text with tree-sitter-tsquery and lower it.
pub fn lower_scm(text: &str) -> Result<ScmProgram, ScmLowerError> {
    let tree = parse_scm(text)?;
    let root = tree.root_node();
    if let Some(broken) = first_broken_node(root) {
        return Err(ScmLowerError::Syntax {
            row: broken.start_position().row as u32,
            message: format!(
                "unparsed `.scm` text: {}",
                first_line(&text[broken.byte_range()])
            ),
        });
    }

    let mut labels = BTreeSet::new();
    let mut labelled = Vec::new();
    let mut plain = Vec::new();
    for definition in top_level_definitions(root) {
        match top_level_label(definition, text) {
            Some(label) => {
                if !labels.insert(label.clone()) {
                    return Err(ScmLowerError::DuplicateLabel(label));
                }
                labelled.push((label, definition));
            }
            None => plain.push(definition),
        }
    }

    let utils = labelled
        .into_iter()
        .map(|(id, definition)| {
            Ok(NamedAstRule {
                id,
                rule: lower_definition(definition, text, &labels)?,
            })
        })
        .collect::<Result<Vec<_>, ScmLowerError>>()?;

    let mut lowered = plain
        .into_iter()
        .map(|definition| lower_definition(definition, text, &labels))
        .collect::<Result<Vec<_>, ScmLowerError>>()?;
    let rule = match lowered.len() {
        0 => AstRule::Any(
            utils
                .iter()
                .map(|named| AstRule::Matches(named.id.clone()))
                .collect(),
        ),
        1 => lowered.remove(0),
        _ => AstRule::Any(lowered),
    };
    Ok(ScmProgram { rule, utils })
}

/// The tree-sitter language for the `.scm` query surface. Its ABI is railed by
/// `tests/144_scm_lower.rs` against the window the tree-sitter runtime accepts.
pub fn scm_language() -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_tsquery::LANGUAGE)
}

fn parse_scm(text: &str) -> Result<Tree, ScmLowerError> {
    let mut parser = Parser::new();
    parser
        .set_language(&scm_language())
        .map_err(|error| ScmLowerError::Parse(error.to_string()))?;
    parser
        .parse(text, None)
        .ok_or_else(|| ScmLowerError::Parse("query grammar produced no tree".into()))
}

/// The grammar's `definition` supertype minus `predicate`: a predicate
/// constrains a definition, so every structural walk skips it.
const DEFINITION_KINDS: [&str; 6] = [
    "named_node",
    "anonymous_node",
    "missing_node",
    "grouping",
    "list",
    "field_definition",
];

fn definition_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    let children = node
        .named_children(&mut cursor)
        .filter(|child| DEFINITION_KINDS.contains(&child.kind()))
        .collect();
    children
}

/// A bare `predicate` is a definition of the `program` and stands alone there,
/// so the top level takes one kind more than a structural walk does.
fn top_level_definitions<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = root.walk();
    let children = root
        .named_children(&mut cursor)
        .filter(|child| DEFINITION_KINDS.contains(&child.kind()) || child.kind() == "predicate")
        .collect();
    children
}

fn first_broken_node<'t>(node: Node<'t>) -> Option<Node<'t>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    if !node.has_error() {
        return None;
    }
    let mut cursor = node.walk();
    let children = node.children(&mut cursor).collect::<Vec<_>>();
    children.into_iter().find_map(first_broken_node)
}

/// The id a top-level definition takes in `utils`. Only a DIRECT `capture`
/// child labels a definition; a deeper capture names a node inside it.
fn top_level_label(definition: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = definition.walk();
    let capture = definition
        .named_children(&mut cursor)
        .find(|child| child.kind() == "capture")?;
    capture_name(capture, source)
}

fn capture_name(capture: Node<'_>, source: &str) -> Option<String> {
    field_child(capture, "name").map(|name| source[name.byte_range()].to_string())
}

/// The first NAMED child under `field`. The grammar fields anonymous tokens too
/// (`#`, `:`), so a plain `child_by_field_name` returns the punctuation.
fn field_child<'t>(node: Node<'t>, field: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node
        .children_by_field_name(field, &mut cursor)
        .find(|child| child.is_named());
    found
}

/// Predicates decide the root: each names the capture it constrains, and that
/// capture's host is what the rule reports. With none, the root is the pattern.
fn lower_definition(
    definition: Node<'_>,
    source: &str,
    labels: &BTreeSet<String>,
) -> Result<AstRule, ScmLowerError> {
    let mut focus: Option<String> = None;
    let mut relations = Vec::new();
    for predicate in predicate_nodes(definition) {
        let (capture, relation) = lower_predicate(predicate, source, labels)?;
        match &focus {
            Some(first) if *first != capture => {
                return Err(ScmLowerError::FocusConflict {
                    first: first.clone(),
                    second: capture,
                })
            }
            _ => focus = Some(capture),
        }
        relations.push(relation);
    }

    let root = pattern_root(definition);
    let Some(focus) = focus else {
        return lower_node(root, source, labels);
    };
    let host = capture_host(definition, &focus, source)
        .ok_or_else(|| ScmLowerError::UnboundReference(focus.clone()))?;
    let mut members = vec![lower_node(host, source, labels)?];
    if host.id() != root.id() {
        members.push(AstRule::Inside {
            rule: Box::new(lower_node(root, source, labels)?),
            stop_by: Some(StopBy::End("end".into())),
        });
    }
    members.extend(relations);
    Ok(AstRule::All(members))
}

/// The node a definition's structure hangs off. A grouping wrapping exactly one
/// pattern beside its predicates constrains nothing, so that pattern is it.
fn pattern_root<'t>(definition: Node<'t>) -> Node<'t> {
    if definition.kind() == "grouping" {
        let children = definition_children(definition);
        if children.len() == 1 {
            return children[0];
        }
    }
    definition
}

fn predicate_nodes<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut found = Vec::new();
    collect_predicates(node, &mut found);
    found
}

fn collect_predicates<'t>(node: Node<'t>, found: &mut Vec<Node<'t>>) {
    if node.kind() == "predicate" {
        found.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_predicates(child, found);
    }
}

/// The node a capture suffixes, which is the capture's parent definition.
fn capture_host<'t>(node: Node<'t>, name: &str, source: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "predicate" {
            continue;
        }
        if child.kind() == "capture" && capture_name(child, source).as_deref() == Some(name) {
            return Some(node);
        }
        if let Some(found) = capture_host(child, name, source) {
            return Some(found);
        }
    }
    None
}

/// A parameter is `capture | identifier | string`, never a nested rule.
/// Recursion arrives by name: an identifier lowers to [`AstRule::Matches`].
fn lower_predicate(
    predicate: Node<'_>,
    source: &str,
    labels: &BTreeSet<String>,
) -> Result<(String, AstRule), ScmLowerError> {
    let name = field_child(predicate, "name")
        .map(|node| source[node.byte_range()].to_string())
        .unwrap_or_default();
    let marker = predicate
        .child_by_field_name("type")
        .map(|node| source[node.byte_range()].to_string())
        .unwrap_or_default();
    let operator = format!("{name}{marker}");

    let parameters = match predicate.child_by_field_name("parameters") {
        Some(node) => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect::<Vec<_>>()
        }
        None => Vec::new(),
    };
    if parameters.len() != 2 {
        return Err(ScmLowerError::PredicateArity {
            operator,
            got: parameters.len(),
        });
    }

    // `operator` above keeps the unpeeled spelling, so an unmapped `#not-foo?`
    // reports itself rather than `foo?`.
    let (name, negated) = match name.strip_prefix("not-") {
        Some(rest) => (rest.to_string(), true),
        None => (name, false),
    };
    let negate = |rule: AstRule| match negated {
        true => AstRule::Not(Box::new(rule)),
        false => rule,
    };

    let relation: fn(Box<AstRule>, Option<StopBy>) -> AstRule = match name.as_str() {
        "inside" => |rule, stop_by| AstRule::Inside { rule, stop_by },
        "has" => |rule, stop_by| AstRule::Has { rule, stop_by },
        "follows" => |rule, stop_by| AstRule::Follows { rule, stop_by },
        "precedes" => |rule, stop_by| AstRule::Precedes { rule, stop_by },
        "match" => {
            let focus = capture_argument(parameters[0], source)?;
            let pattern = string_argument(parameters[1], source)?;
            return Ok((focus, negate(AstRule::Regex(pattern))));
        }
        _ => return Err(ScmLowerError::UnknownPredicate(operator)),
    };

    let focus = capture_argument(parameters[0], source)?;
    let reference = reference_argument(parameters[1], source, labels)?;
    Ok((
        focus,
        negate(relation(
            Box::new(AstRule::Matches(reference)),
            Some(StopBy::End("end".into())),
        )),
    ))
}

fn capture_argument(node: Node<'_>, source: &str) -> Result<String, ScmLowerError> {
    if node.kind() != "capture" {
        return Err(ScmLowerError::UnboundReference(
            source[node.byte_range()].to_string(),
        ));
    }
    capture_name(node, source)
        .ok_or_else(|| ScmLowerError::UnboundReference(source[node.byte_range()].to_string()))
}

fn reference_argument(
    node: Node<'_>,
    source: &str,
    labels: &BTreeSet<String>,
) -> Result<String, ScmLowerError> {
    let text = source[node.byte_range()].to_string();
    if node.kind() != "identifier" || !labels.contains(&text) {
        return Err(ScmLowerError::UnboundReference(text));
    }
    Ok(text)
}

fn string_argument(node: Node<'_>, source: &str) -> Result<String, ScmLowerError> {
    if node.kind() != "string" {
        return Err(ScmLowerError::UnboundReference(
            source[node.byte_range()].to_string(),
        ));
    }
    let mut cursor = node.walk();
    let content = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "string_content")
        .map(|content| source[content.byte_range()].to_string())
        .unwrap_or_default();
    Ok(content)
}

fn lower_node(
    node: Node<'_>,
    source: &str,
    labels: &BTreeSet<String>,
) -> Result<AstRule, ScmLowerError> {
    match node.kind() {
        "named_node" => lower_named_node(node, source, labels),
        "anonymous_node" => Ok(AstRule::Kind(anonymous_kind(node, source)?)),
        "list" => Ok(AstRule::Any(
            definition_children(node)
                .into_iter()
                .map(|child| lower_node(child, source, labels))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        "grouping" => Ok(AstRule::All(
            definition_children(node)
                .into_iter()
                .map(|child| lower_node(child, source, labels))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        "field_definition" => {
            let inner = definition_children(node)
                .into_iter()
                .next()
                .ok_or_else(|| unrepresentable(node, "a field definition with no pattern"))?;
            lower_node(inner, source, labels)
        }
        _ => Err(unrepresentable(node, node.kind())),
    }
}

/// Kind, plus a `Has` per nested pattern and a `Not(Has)` per negated field.
fn lower_named_node(
    node: Node<'_>,
    source: &str,
    labels: &BTreeSet<String>,
) -> Result<AstRule, ScmLowerError> {
    let name = field_child(node, "name")
        .ok_or_else(|| unrepresentable(node, "a wildcard node `_`"))?;
    let kind = match name.kind() {
        "string" => string_argument(name, source)?,
        _ => source[name.byte_range()].to_string(),
    };

    let mut cursor = node.walk();
    let children = node.named_children(&mut cursor).collect::<Vec<_>>();
    let mut members = vec![AstRule::Kind(kind)];
    for child in children {
        if child.kind() == "negated_field" {
            members.push(AstRule::Not(Box::new(AstRule::Has {
                rule: Box::new(AstRule::Kind(negated_field_name(child, source))),
                stop_by: None,
            })));
        } else if DEFINITION_KINDS.contains(&child.kind()) {
            members.push(AstRule::Has {
                rule: Box::new(lower_node(child, source, labels)?),
                stop_by: None,
            });
        }
    }
    Ok(if members.len() == 1 {
        members.remove(0)
    } else {
        AstRule::All(members)
    })
}

fn negated_field_name(node: Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    let name = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier")
        .map(|child| source[child.byte_range()].to_string())
        .unwrap_or_default();
    name
}

/// An anonymous node names a literal token, and a tree-sitter token's kind is
/// its own text, so the quoted spelling is the kind.
fn anonymous_kind(node: Node<'_>, source: &str) -> Result<String, ScmLowerError> {
    match field_child(node, "name") {
        Some(name) if name.kind() == "string" => string_argument(name, source),
        _ => Err(unrepresentable(node, "a wildcard node `_`")),
    }
}

fn unrepresentable(node: Node<'_>, what: &str) -> ScmLowerError {
    ScmLowerError::Syntax {
        row: node.start_position().row as u32,
        message: format!("{what} has no AstRule"),
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}
