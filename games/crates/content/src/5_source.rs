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

/// One decomp call already represented by current game semantics.
///
/// The tuple is `(callee symbol, game operation, owning crate)`. The operation
/// names are the vocabulary the Rust side exposes: `transition`,
/// `animation_finished`, `crouch_request`, `crouch_release`, `jump_request`,
/// `motion_transition` and friend come from `game-fighter`; `turn_guard`,
/// `jump_choice` and `air_jump_choice` are the source guards `smash-import`
/// lowers through `game-content`. Everything not listed here is unsupported
/// and is reported with its source path, line and symbol.
pub const RECOGNIZED_OPERATIONS: &[(&str, &str, &str)] = &[
    ("Fighter_ChangeMotionState", "transition", "game-fighter"),
    ("ftAnim_IsFramesRemaining", "animation_finished", "game-fighter"),
    ("ft_8008A348", "motion_transition", "game-fighter"),
    ("ft_8008A2BC", "motion_transition", "game-fighter"),
    ("ftCo_800D5FB0", "crouch_enter_dispatch", "game-fighter"),
    ("ftCo_800D638C", "crouch_hold_enter", "game-fighter"),
    ("ftCo_Squat_CheckInput", "crouch_request", "game-fighter"),
    ("ftCo_SquatRv_CheckInput", "crouch_release", "game-fighter"),
    ("ftCo_Jump_CheckInput", "jump_request", "game-fighter"),
    ("getAccelAndTarget", "dash_run_acceleration", "game-fighter"),
    ("ftCo_800C97A8", "turn_guard", "game-content"),
    ("ftCo_Jump_Enter", "jump_choice", "game-content"),
    ("ftCo_JumpAerial_Enter_Basic", "air_jump_choice", "game-content"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSite {
    /// Index into [`Inventory::files`].
    pub file: usize,
    pub line: usize,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    /// Index into [`Inventory::files`].
    pub file: usize,
    pub line: usize,
    /// Index into [`Inventory::functions`]; direct calls always have an owner.
    pub function: usize,
    pub symbol: String,
    /// Index into [`RECOGNIZED_OPERATIONS`], or `None` when unsupported.
    pub operation: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    /// Sorted repository-relative source paths.
    pub files: Vec<String>,
    /// Indices into `files` whose syntax tree contains error nodes.
    pub parse_errors: Vec<usize>,
    pub functions: Vec<FunctionSite>,
    /// Stable-sorted by `(file, line, function, symbol)`.
    pub calls: Vec<CallSite>,
}

impl Inventory {
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub fn call_count(&self) -> usize {
        self.calls.len()
    }

    pub fn recognized_count(&self) -> usize {
        self.calls.iter().filter(|call| call.operation.is_some()).count()
    }

    pub fn unsupported_count(&self) -> usize {
        self.call_count() - self.recognized_count()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SourceError {
    Parse,
    MissingFunction(String),
    MissingNode { function: String, kind: &'static str },
    UnsupportedExpression(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SourceError {}

fn tree(source: &str) -> Result<tree_sitter::Tree, SourceError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|_| SourceError::Parse)?;
    let tree = parser.parse(source, None).ok_or(SourceError::Parse)?;
    (!tree.root_node().has_error())
        .then_some(tree)
        .ok_or(SourceError::Parse)
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
    let rhs = node.child_by_field_name("right").ok_or(SourceError::Parse)?;
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
    Ok((compare(condition, source)?, function.start_position().row + 1))
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
    let condition = choice.child_by_field_name("condition").ok_or(SourceError::Parse)?;
    let consequence = choice.child_by_field_name("consequence").ok_or(SourceError::Parse)?;
    let alternative = choice.child_by_field_name("alternative").ok_or(SourceError::Parse)?;
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

fn declared_name(node: Node<'_>, source: &str) -> String {
    let Some(declarator) = node.child_by_field_name("declarator") else {
        return String::new();
    };
    let mut name = String::new();
    visit(declarator, &mut |part| {
        if name.is_empty() && part.kind() == "identifier" {
            name = text(part, source).into();
        }
    });
    name
}

/// Collect direct `identifier(...)` calls and every function definition under
/// `node`. Non-identifier callees (field, subscript, parenthesised) are
/// indirect and stay out of the inventory.
fn collect(
    node: Node<'_>,
    source: &str,
    file: usize,
    current: Option<usize>,
    functions: &mut Vec<FunctionSite>,
    calls: &mut Vec<CallSite>,
) {
    let mut current = current;
    if node.kind() == "function_definition" {
        functions.push(FunctionSite {
            file,
            line: node.start_position().row + 1,
            name: declared_name(node, source),
        });
        current = Some(functions.len() - 1);
    } else if node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
        && callee.kind() == "identifier"
        && let Some(function) = current
    {
        calls.push(CallSite {
            file,
            line: node.start_position().row + 1,
            function,
            symbol: text(callee, source).into(),
            operation: None,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, source, file, current, functions, calls);
    }
}

/// Inventory every function definition and direct call expression in a bounded
/// set of C translation units. `sources` is `(relative path, source text)`;
/// paths are sorted before indexing so `file` indices are stable.
///
/// Parse errors do not abort the walk. The recovered tree still contributes
/// functions and calls, and the file index is recorded in
/// [`Inventory::parse_errors`] so the caller reports it rather than silently
/// trusting the tree.
pub fn common_inventory(sources: &[(String, String)]) -> Result<Inventory, SourceError> {
    let mut ordered: Vec<&(String, String)> = sources.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|_| SourceError::Parse)?;

    let mut inventory = Inventory {
        files: Vec::new(),
        parse_errors: Vec::new(),
        functions: Vec::new(),
        calls: Vec::new(),
    };
    for (file, (path, source)) in ordered.into_iter().enumerate() {
        inventory.files.push(path.clone());
        let tree = parser.parse(source, None).ok_or(SourceError::Parse)?;
        if tree.root_node().has_error() {
            inventory.parse_errors.push(file);
        }
        collect(
            tree.root_node(),
            source,
            file,
            None,
            &mut inventory.functions,
            &mut inventory.calls,
        );
    }

    inventory.calls.sort_by(|a, b| {
        (a.file, a.line, a.function, a.symbol.as_str()).cmp(&(
            b.file,
            b.line,
            b.function,
            b.symbol.as_str(),
        ))
    });
    for call in &mut inventory.calls {
        call.operation = RECOGNIZED_OPERATIONS
            .iter()
            .position(|(symbol, _, _)| *symbol == call.symbol);
    }
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
        assert_eq!(function_evidence(SOURCE, "choose").unwrap().calls, ["change"]);
    }

    const INVENTORY_SOURCE: &str = r#"
int helper(int x) {
    return x;
}
void first(Fighter* fp) {
    if (ftAnim_IsFramesRemaining(fp->gobj)) {
        Fighter_ChangeMotionState(fp->gobj, 1, 0, 0, 0, 0, 0);
    }
    helper(1);
    fp->callback(2);
}
"#;

    fn inventory(sources: &[(&str, &str)]) -> Inventory {
        common_inventory(
            &sources
                .iter()
                .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn inventories_functions_calls_and_recognized_operations() {
        let records = inventory(&[("a.c", INVENTORY_SOURCE)]);
        assert_eq!(records.files, ["a.c"]);
        assert_eq!(
            records.functions.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["helper", "first"],
        );
        assert_eq!(
            records
                .calls
                .iter()
                .map(|c| c.symbol.as_str())
                .collect::<Vec<_>>(),
            ["ftAnim_IsFramesRemaining", "Fighter_ChangeMotionState", "helper"],
        );
        assert_eq!(records.calls.iter().map(|c| c.function).collect::<Vec<_>>(), [1, 1, 1]);
        assert_eq!((records.function_count(), records.call_count()), (2, 3));
        assert_eq!((records.recognized_count(), records.unsupported_count()), (2, 1));
        let unsupported = records.calls.iter().find(|c| c.operation.is_none()).unwrap();
        assert_eq!(unsupported.symbol, "helper");
        assert!(!records.parse_errors.contains(&0));
    }

    #[test]
    fn sorts_files_by_path_before_indexing() {
        let records = inventory(&[("b.c", INVENTORY_SOURCE), ("a.c", "void a(void) {}\n")]);
        assert_eq!(records.files, ["a.c", "b.c"]);
        assert!(records.functions.iter().all(|f| f.file != 0 || f.name == "a"));
    }

    #[test]
    fn flags_parse_error_files_without_aborting() {
        let records = inventory(&[("broken.c", "void f(void) { g( }\n")]);
        assert_eq!(records.parse_errors, [0]);
    }
}
