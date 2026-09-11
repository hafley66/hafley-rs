use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guard {
    pub lhs: String,
    #[serde(rename = "operator")]
    pub op: Op,
    pub rhs: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub repository: String,
    pub revision: String,
    pub path: String,
    pub line: usize,
    pub symbol: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRule {
    pub from: String,
    pub event: String,
    pub to: String,
    pub guard: Guard,
    pub source: SourceRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unresolved {
    pub symbol: String,
    pub source: SourceRef,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionEvidence {
    pub line: usize,
    pub calls: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SourceError {
    Parse,
    MissingFunction(String),
    MissingNode {
        function: String,
        kind: &'static str,
    },
    UnsupportedExpression(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SourceError {}

fn tree(source: &str) -> Result<tree_sitter::Tree, SourceError> {
    let tree = parse(source)?;
    (!tree.root_node().has_error())
        .then_some(tree)
        .ok_or(SourceError::Parse)
}

fn parse(source: &str) -> Result<tree_sitter::Tree, SourceError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|_| SourceError::Parse)?;
    parser.parse(source, None).ok_or(SourceError::Parse)
}

/// Removes two repository facts the C grammar cannot see: the `UNUSED` macro
/// expands to nothing (melee/src/Runtime/platform.h `#else #define UNUSED`) and
/// `MUST_MATCH` is undefined in the whole repository, so its `#ifdef` blocks
/// are inactive in every build. Lines are blanked, never deleted, so source
/// line numbers stay stable.
pub fn preprocess(source: &str) -> String {
    let mut output = Vec::with_capacity(source.lines().count());
    let mut skipping = false;
    for line in source.lines() {
        if line.trim_start().starts_with("#ifdef MUST_MATCH") {
            skipping = true;
            output.push(String::new());
        } else if skipping && line.trim_start().starts_with("#endif") {
            skipping = false;
            output.push(String::new());
        } else if skipping {
            output.push(String::new());
        } else {
            output.push(line.replace("UNUSED ", ""));
        }
    }
    output.push(String::new());
    output.join("\n")
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn visit<'tree>(node: Node<'tree>, f: &mut impl FnMut(Node<'tree>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, f);
    }
}

fn function<'a>(root: Node<'a>, source: &str, name: &str) -> Result<Node<'a>, SourceError> {
    let mut found = None;
    visit(root, &mut |node| {
        if found.is_some() || node.kind() != "function_definition" {
            return;
        }
        if let Some(declarator) = node.child_by_field_name("declarator") {
            let mut matches = false;
            visit(declarator, &mut |part| {
                matches |= part.kind() == "identifier" && text(part, source) == name;
            });
            if matches {
                found = Some(node);
            }
        }
    });
    found.ok_or_else(|| SourceError::MissingFunction(name.into()))
}

fn canonical(node: Node<'_>, source: &str) -> String {
    if node.kind() == "parenthesized_expression"
        && let Some(inner) = node.named_child(0)
    {
        return canonical(inner, source);
    }
    text(node, source)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn compare(node: Node<'_>, source: &str) -> Result<Guard, SourceError> {
    let node = if node.kind() == "parenthesized_expression" {
        node.named_child(0).ok_or(SourceError::Parse)?
    } else {
        node
    };
    if node.kind() != "binary_expression" {
        return Err(SourceError::UnsupportedExpression(canonical(node, source)));
    }
    let lhs = node.child_by_field_name("left").ok_or(SourceError::Parse)?;
    let rhs = node
        .child_by_field_name("right")
        .ok_or(SourceError::Parse)?;
    let operator = node
        .child_by_field_name("operator")
        .map(|operator| text(operator, source))
        .or_else(|| {
            let between = &source[lhs.end_byte()..rhs.start_byte()];
            ["<=", ">=", "==", "!=", "<", ">"]
                .into_iter()
                .find(|operator| between.contains(operator))
        })
        .ok_or_else(|| SourceError::UnsupportedExpression(canonical(node, source)))?;
    let op = match operator {
        "<" => Op::Less,
        "<=" => Op::LessEqual,
        ">" => Op::Greater,
        ">=" => Op::GreaterEqual,
        "==" => Op::Equal,
        "!=" => Op::NotEqual,
        _ => return Err(SourceError::UnsupportedExpression(operator.into())),
    };
    Ok(Guard {
        lhs: canonical(lhs, source),
        op,
        rhs: canonical(rhs, source),
    })
}

pub fn if_guard(source: &str, name: &str) -> Result<(Guard, usize), SourceError> {
    let tree = tree(source)?;
    let function = function(tree.root_node(), source, name)?;
    let mut condition = None;
    visit(function, &mut |node| {
        if condition.is_none() && node.kind() == "if_statement" {
            condition = node.child_by_field_name("condition");
        }
    });
    let condition = condition.ok_or_else(|| SourceError::MissingNode {
        function: name.into(),
        kind: "if_statement",
    })?;
    Ok((
        compare(condition, source)?,
        function.start_position().row + 1,
    ))
}

pub fn conditional_choice(
    source: &str,
    name: &str,
) -> Result<(Guard, String, String, usize), SourceError> {
    let tree = tree(source)?;
    let function = function(tree.root_node(), source, name)?;
    let mut choice = None;
    visit(function, &mut |node| {
        if choice.is_none() && node.kind() == "conditional_expression" {
            choice = Some(node);
        }
    });
    let choice = choice.ok_or_else(|| SourceError::MissingNode {
        function: name.into(),
        kind: "conditional_expression",
    })?;
    let condition = choice
        .child_by_field_name("condition")
        .ok_or(SourceError::Parse)?;
    let consequence = choice
        .child_by_field_name("consequence")
        .ok_or(SourceError::Parse)?;
    let alternative = choice
        .child_by_field_name("alternative")
        .ok_or(SourceError::Parse)?;
    Ok((
        compare(condition, source)?,
        canonical(consequence, source),
        canonical(alternative, source),
        function.start_position().row + 1,
    ))
}

pub fn function_evidence(source: &str, name: &str) -> Result<FunctionEvidence, SourceError> {
    let tree = tree(source)?;
    let function = function(tree.root_node(), source, name)?;
    let mut calls = Vec::new();
    visit(function, &mut |node| {
        if node.kind() == "call_expression"
            && let Some(callee) = node.child_by_field_name("function")
        {
            calls.push(canonical(callee, source));
        }
    });
    Ok(FunctionEvidence {
        line: function.start_position().row + 1,
        calls,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSite {
    pub path: String,
    pub line: usize,
    pub symbol: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub path: String,
    pub line: usize,
    pub function: String,
    pub callee: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub files: usize,
    pub functions: Vec<FunctionSite>,
    pub calls: Vec<CallSite>,
    pub incomplete: Vec<String>,
}

fn contains_error(node: Node<'_>) -> bool {
    let mut found = false;
    visit(node, &mut |node| {
        found |= node.is_error() || node.is_missing();
    });
    found
}

fn definition_name<'a>(definition: Node<'_>, source: &'a str) -> Option<&'a str> {
    let declarator = definition.child_by_field_name("declarator")?;
    let mut name = None;
    visit(declarator, &mut |node| {
        if name.is_none()
            && node.kind() == "function_declarator"
            && let Some(target) = node.child_by_field_name("declarator")
            && target.kind() == "identifier"
        {
            name = Some(text(target, source));
        }
    });
    name
}

pub fn inventory_file(source: &str, path: &str) -> Result<Inventory, SourceError> {
    let source = preprocess(source);
    let tree = parse(&source)?;
    let mut inventory = Inventory::default();
    inventory.files = 1;
    if tree.root_node().has_error() {
        inventory.incomplete.push(path.into());
    }
    visit(tree.root_node(), &mut |node| {
        if node.kind() != "function_definition" || contains_error(node) {
            return;
        }
        let Some(symbol) = definition_name(node, &source) else {
            return;
        };
        let symbol = symbol.to_string();
        inventory.functions.push(FunctionSite {
            path: path.into(),
            line: node.start_position().row + 1,
            symbol: symbol.clone(),
        });
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut calls = Vec::new();
        visit(body, &mut |node| {
            if node.kind() == "call_expression"
                && let Some(callee) = node.child_by_field_name("function")
            {
                calls.push((node.start_position().row + 1, canonical(callee, &source)));
            }
        });
        inventory
            .calls
            .extend(calls.into_iter().map(|(line, callee)| CallSite {
                path: path.into(),
                line,
                function: symbol.clone(),
                callee,
            }));
    });
    Ok(inventory)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
bool threshold(Fighter* fp) {
    if (fp->input.x * fp->facing <= common->x34) { return true; }
    return false;
}
void choose(Fighter* fp) {
    state = (fp->input.x * fp->facing) > -common->x78 ? Forward : Backward;
    change(state);
}
"#;

    #[test]
    fn lowers_comparisons_from_c_syntax_nodes() {
        assert_eq!(
            if_guard(SOURCE, "threshold").unwrap().0,
            Guard {
                lhs: "fp->input.x*fp->facing".into(),
                op: Op::LessEqual,
                rhs: "common->x34".into(),
            },
        );
        assert_eq!(
            conditional_choice(SOURCE, "choose").unwrap(),
            (
                Guard {
                    lhs: "fp->input.x*fp->facing".into(),
                    op: Op::Greater,
                    rhs: "-common->x78".into(),
                },
                "Forward".into(),
                "Backward".into(),
                6,
            ),
        );
    }

    #[test]
    fn records_calls_without_text_pattern_matching() {
        assert_eq!(
            function_evidence(SOURCE, "choose").unwrap().calls,
            ["change"]
        );
    }

    const MULTI: &str = r#"
void helper(void);
void caller(Fighter* fp) {
    helper();
    if (fp->x > 0) { common->enter(fp); }
}
int spot(void) { return spot(); }
"#;

    #[test]
    fn preprocess_blanks_inactive_macros_and_preserves_lines() {
        let source = "#define X 1\n#ifdef MUST_MATCH\n    kb_mag = 0;\n#endif\nUNUSED u8 _[8];\n";
        assert_eq!(preprocess(source), "#define X 1\n\n\n\nu8 _[8];\n");
        assert_eq!(preprocess(source).lines().count(), source.lines().count());
    }

    #[test]
    fn inventories_definitions_and_direct_calls() {
        let inventory = inventory_file(MULTI, "ftCo_X.c").unwrap();
        assert_eq!(inventory.files, 1);
        assert_eq!(
            inventory.functions,
            [
                FunctionSite {
                    path: "ftCo_X.c".into(),
                    line: 3,
                    symbol: "caller".into()
                },
                FunctionSite {
                    path: "ftCo_X.c".into(),
                    line: 7,
                    symbol: "spot".into()
                },
            ],
        );
        assert_eq!(
            inventory.calls,
            [
                CallSite {
                    path: "ftCo_X.c".into(),
                    line: 4,
                    function: "caller".into(),
                    callee: "helper".into()
                },
                CallSite {
                    path: "ftCo_X.c".into(),
                    line: 5,
                    function: "caller".into(),
                    callee: "common->enter".into()
                },
                CallSite {
                    path: "ftCo_X.c".into(),
                    line: 7,
                    function: "spot".into(),
                    callee: "spot".into()
                },
            ],
        );
    }

    const MACROS: &str = r#"
#ifdef MUST_MATCH
    kb_mag = 0;
#endif
static inline void squashed(Fighter* fp) {
    UNUSED u8 _[8];
    helper(fp);
}
"#;

    #[test]
    fn inventories_after_preprocessing_without_offset_drift() {
        let inventory = inventory_file(MACROS, "ftCo_M.c").unwrap();
        assert_eq!(
            inventory.functions,
            [FunctionSite {
                path: "ftCo_M.c".into(),
                line: 5,
                symbol: "squashed".into()
            }],
        );
        assert_eq!(
            inventory.calls,
            [CallSite {
                path: "ftCo_M.c".into(),
                line: 7,
                function: "squashed".into(),
                callee: "helper".into(),
            }],
        );
    }
}
