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

/// One value on the generated port's typed boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortValue {
    /// A query field read from the fighter, e.g. `fp->input.lstick[0].x`.
    Query(String),
    /// A raw `p_ftCommonData` field. The numeric value is not retained, so the
    /// generated Rust exposes it as a typed input field rather than a constant.
    Common(String),
    /// A decomp motion id (`ftCo_MS_*`), lowered to a typed action.
    Action(String),
}

/// A pure expression inside the translated subset. Anything outside the subset
/// is rejected with [`SourceError::UnsupportedExpression`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortExpr {
    Value(PortValue),
    Neg(Box<PortExpr>),
    Mul(Box<PortExpr>, Box<PortExpr>),
    Compare(Box<PortExpr>, Op, Box<PortExpr>),
}

/// 1-based line and column extent of a translated node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortSpan {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortBody {
    /// `return <comparison>;`, e.g. `ftCo_800C97A8`.
    Guard(PortExpr),
    /// `x = <condition> ? <yes> : <no>;`, e.g. `ftCo_Jump_Enter`.
    Choice {
        condition: PortExpr,
        yes: PortExpr,
        no: PortExpr,
    },
}

/// One lowered decomp callback plus the provenance the generated file retains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortFn {
    pub name: String,
    pub path: String,
    pub function_line: usize,
    pub function_end_line: usize,
    /// Span of the decision expression, not the whole function.
    pub span: PortSpan,
    /// Direct calls the translator does not translate; reported explicitly.
    pub calls: Vec<String>,
    pub body: PortBody,
}

/// Inputs for [`emit_port_rust`].
pub struct PortFile<'a> {
    pub repository: &'a str,
    pub revision: &'a str,
    pub functions: &'a [PortFn],
}

fn parse_op(operator: &str) -> Option<Op> {
    Some(match operator {
        "<" => Op::Less,
        "<=" => Op::LessEqual,
        ">" => Op::Greater,
        ">=" => Op::GreaterEqual,
        "==" => Op::Equal,
        "!=" => Op::NotEqual,
        _ => return None,
    })
}

fn valid_ident(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

fn skip_parens<'a>(node: Node<'a>, _source: &str) -> Node<'a> {
    let mut node = node;
    while node.kind() == "parenthesized_expression" {
        match node.named_child(0) {
            Some(inner) => node = inner,
            None => break,
        }
    }
    node
}

fn binary_operator(node: Node<'_>, source: &str) -> Option<String> {
    if let Some(operator) = node.child_by_field_name("operator") {
        return Some(text(operator, source).into());
    }
    let lhs = node.child_by_field_name("left")?;
    let rhs = node.child_by_field_name("right")?;
    let between = &source[lhs.end_byte()..rhs.start_byte()];
    ["<=", ">=", "==", "!=", "<", ">", "*", "+", "-", "/"]
        .into_iter()
        .find(|operator| between.contains(operator))
        .map(Into::into)
}

fn unary_operator(node: Node<'_>, source: &str) -> Option<String> {
    if let Some(operator) = node.child_by_field_name("operator") {
        return Some(text(operator, source).trim().into());
    }
    let argument = node.named_child(0)?;
    Some(source[node.start_byte()..argument.start_byte()].trim().into())
}

fn port_value(canonical_text: &str) -> Option<PortValue> {
    match canonical_text {
        "fp->input.lstick[0].x" => return Some(PortValue::Query("lstick_x".into())),
        "fp->facing_dir" => return Some(PortValue::Query("facing_dir".into())),
        _ => {}
    }
    if let Some(field) = canonical_text.strip_prefix("p_ftCommonData->") {
        if valid_ident(field) {
            return Some(PortValue::Common(field.into()));
        }
    }
    if let Some(action) = canonical_text.strip_prefix("ftCo_MS_") {
        if valid_ident(action) {
            return Some(PortValue::Action(action.into()));
        }
    }
    None
}

fn lower_expr(node: Node<'_>, source: &str) -> Result<PortExpr, SourceError> {
    let node = skip_parens(node, source);
    match node.kind() {
        "unary_expression" => {
            let operator = unary_operator(node, source)
                .ok_or_else(|| SourceError::UnsupportedExpression(canonical(node, source)))?;
            if operator != "-" {
                return Err(SourceError::UnsupportedExpression(canonical(node, source)));
            }
            let argument = node
                .child_by_field_name("argument")
                .or_else(|| node.named_child(0))
                .ok_or(SourceError::Parse)?;
            Ok(PortExpr::Neg(Box::new(lower_expr(argument, source)?)))
        }
        "binary_expression" => {
            let lhs = node.child_by_field_name("left").ok_or(SourceError::Parse)?;
            let rhs = node.child_by_field_name("right").ok_or(SourceError::Parse)?;
            let operator = binary_operator(node, source)
                .ok_or_else(|| SourceError::UnsupportedExpression(canonical(node, source)))?;
            match operator.as_str() {
                "*" => Ok(PortExpr::Mul(
                    Box::new(lower_expr(lhs, source)?),
                    Box::new(lower_expr(rhs, source)?),
                )),
                _ => {
                    let op = parse_op(&operator)
                        .ok_or_else(|| SourceError::UnsupportedExpression(operator.clone()))?;
                    Ok(PortExpr::Compare(
                        Box::new(lower_expr(lhs, source)?),
                        op,
                        Box::new(lower_expr(rhs, source)?),
                    ))
                }
            }
        }
        _ => {
            let value = canonical(node, source);
            port_value(&value)
                .map(PortExpr::Value)
                .ok_or(SourceError::UnsupportedExpression(value))
        }
    }
}

fn span_of(node: Node<'_>) -> PortSpan {
    let start = node.start_position();
    let end = node.end_position();
    PortSpan {
        line: start.row + 1,
        column: start.column + 1,
        end_line: end.row + 1,
        end_column: end.column + 1,
    }
}

/// Calls that are the typed boundary itself rather than untranslated engine
/// work: `GET_FIGHTER` yields the fighter the query reads.
const BOUNDARY_CALLS: &[&str] = &["GET_FIGHTER"];

fn port_fn(
    source: &str,
    path: &str,
    name: &str,
    function: Node<'_>,
    decision: Node<'_>,
    body: PortBody,
) -> Result<PortFn, SourceError> {
    let mut calls = function_evidence(source, name)?.calls;
    calls.retain(|call| !BOUNDARY_CALLS.contains(&call.as_str()));
    Ok(PortFn {
        name: name.into(),
        path: path.into(),
        function_line: function.start_position().row + 1,
        function_end_line: function.end_position().row + 1,
        span: span_of(decision),
        calls,
        body,
    })
}

/// Lower a callback whose decision is the first `if` condition, e.g.
/// `ftCo_800C97A8`. The body becomes a pure boolean guard.
pub fn lower_guard(source: &str, path: &str, name: &str) -> Result<PortFn, SourceError> {
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
    let expr = lower_expr(condition, source)?;
    port_fn(source, path, name, function, condition, PortBody::Guard(expr))
}

/// Lower a callback whose decision is the first conditional expression, e.g.
/// `ftCo_Jump_Enter`. The body becomes a pure action choice.
pub fn lower_choice(source: &str, path: &str, name: &str) -> Result<PortFn, SourceError> {
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
    let yes = choice.child_by_field_name("consequence").ok_or(SourceError::Parse)?;
    let no = choice.child_by_field_name("alternative").ok_or(SourceError::Parse)?;
    let body = PortBody::Choice {
        condition: lower_expr(condition, source)?,
        yes: lower_expr(yes, source)?,
        no: lower_expr(no, source)?,
    };
    port_fn(source, path, name, function, choice, body)
}

fn op_text(op: Op) -> &'static str {
    match op {
        Op::Less => "<",
        Op::LessEqual => "<=",
        Op::Greater => ">",
        Op::GreaterEqual => ">=",
        Op::Equal => "==",
        Op::NotEqual => "!=",
    }
}

/// Render a lowered expression back to the C spelling it came from so the
/// generated Rust keeps the decomp identifiers searchable.
fn source_expr(expr: &PortExpr) -> String {
    match expr {
        PortExpr::Value(PortValue::Query(field)) => match field.as_str() {
            "lstick_x" => "fp->input.lstick[0].x".into(),
            "facing_dir" => "fp->facing_dir".into(),
            other => format!("query.{other}"),
        },
        PortExpr::Value(PortValue::Common(field)) => format!("p_ftCommonData->{field}"),
        PortExpr::Value(PortValue::Action(name)) => format!("ftCo_MS_{name}"),
        PortExpr::Neg(inner) => format!("-{}", source_expr(inner)),
        PortExpr::Mul(lhs, rhs) => format!("{}*{}", source_expr(lhs), source_expr(rhs)),
        PortExpr::Compare(lhs, op, rhs) => {
            format!("{} {} {}", source_expr(lhs), op_text(*op), source_expr(rhs))
        }
    }
}

fn emit_expr(expr: &PortExpr) -> String {
    match expr {
        PortExpr::Value(PortValue::Query(field)) => format!("query.{field}"),
        PortExpr::Value(PortValue::Common(field)) => format!("common.{field}"),
        PortExpr::Value(PortValue::Action(name)) => format!("FtMotionId::{name}"),
        PortExpr::Neg(inner) => match **inner {
            PortExpr::Value(_) => format!("-{}", emit_expr(inner)),
            _ => format!("-({})", emit_expr(inner)),
        },
        PortExpr::Mul(lhs, rhs) => format!("({} * {})", emit_expr(lhs), emit_expr(rhs)),
        PortExpr::Compare(lhs, op, rhs) => {
            format!("({} {} {})", emit_expr(lhs), op_text(*op), emit_expr(rhs))
        }
    }
}

/// Render lowered callbacks as a Rust module. Names, provenance and unresolved
/// external calls are retained; no source value is invented.
pub fn emit_port_rust(file: &PortFile<'_>) -> Result<String, SourceError> {
    let mut output = String::from(
        "// @generated by smash-import from the pinned Melee decomp.\n\
         // Do not edit by hand; run `just source-rules`.\n",
    );
    output.push_str(&format!("// repository: {}\n", file.repository));
    output.push_str(&format!("// revision: {}\n\n", file.revision));
    output.push_str("#![allow(non_snake_case)]\n\n");
    output.push_str("use crate::{CommonData, FighterQuery, FtMotionId};\n");
    for function in file.functions {
        output.push('\n');
        output.push_str(&format!(
            "/// C source: `{}` in `{}` (function lines {}-{}).\n",
            function.name, function.path, function.function_line, function.function_end_line,
        ));
        output.push_str(&format!(
            "/// Guard span: {}:{}:{}-{}:{}.\n",
            function.path,
            function.span.line,
            function.span.column,
            function.span.end_line,
            function.span.end_column,
        ));
        if function.calls.is_empty() {
            output.push_str("/// External calls: none.\n");
        } else {
            output.push_str(&format!(
                "/// Untranslated external calls: {}.\n",
                function.calls.join(", "),
            ));
        }
        match &function.body {
            PortBody::Guard(expr) => {
                output.push_str(&format!("/// Source guard: `{}`.\n", source_expr(expr)));
                output.push_str(&format!(
                    "pub fn {}(query: &FighterQuery, common: &CommonData) -> bool {{\n    {}\n}}\n",
                    function.name,
                    emit_expr(expr),
                ));
            }
            PortBody::Choice { condition, yes, no } => {
                output.push_str(&format!(
                    "/// Source decision: `{} ? {} : {}`.\n",
                    source_expr(condition),
                    source_expr(yes),
                    source_expr(no),
                ));
                output.push_str(&format!(
                    "pub fn {}(query: &FighterQuery, common: &CommonData) -> FtMotionId {{\n    \
                     if {} {{\n        {}\n    }} else {{\n        {}\n    }}\n}}\n",
                    function.name,
                    emit_expr(condition),
                    emit_expr(yes),
                    emit_expr(no),
                ));
            }
        }
    }
    Ok(output)
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

    const PORT_SOURCE: &str = r#"
bool ftCo_800C97A8(Fighter_GObj* gobj) {
    Fighter* fp = GET_FIGHTER(gobj);
    if (fp->input.lstick[0].x * fp->facing_dir <= p_ftCommonData->x34) {
        return true;
    }
    return false;
}

void ftCo_Jump_Enter(Fighter_GObj* gobj) {
    Fighter* fp = GET_FIGHTER(gobj);
    FtMotionId msid;
    ftCommon_8007D5D4(fp);
    msid = (fp->input.lstick[0].x * fp->facing_dir) > -p_ftCommonData->x78
               ? ftCo_MS_JumpF
               : ftCo_MS_JumpB;
    Fighter_ChangeMotionState(gobj, msid, 0, 0.0F, 1.0F, 0.0F, NULL);
    fp->x2227_b0 = true;
}
"#;

    fn port_file(functions: &[PortFn]) -> String {
        emit_port_rust(&PortFile {
            repository: "https://example.invalid/melee.git",
            revision: "0123456789abcdef0123456789abcdef01234567",
            functions,
        })
        .unwrap()
    }

    #[test]
    fn lowers_guard_and_choice_to_typed_boundary_values() {
        let guard = lower_guard(PORT_SOURCE, "ftCo_Turn.c", "ftCo_800C97A8").unwrap();
        assert_eq!(guard.calls, Vec::<String>::new());
        assert_eq!(
            guard.body,
            PortBody::Guard(PortExpr::Compare(
                Box::new(PortExpr::Mul(
                    Box::new(PortExpr::Value(PortValue::Query("lstick_x".into()))),
                    Box::new(PortExpr::Value(PortValue::Query("facing_dir".into()))),
                )),
                Op::LessEqual,
                Box::new(PortExpr::Value(PortValue::Common("x34".into()))),
            )),
        );

        let choice = lower_choice(PORT_SOURCE, "ftCo_Jump.c", "ftCo_Jump_Enter").unwrap();
        assert_eq!(
            choice.calls,
            ["ftCommon_8007D5D4", "Fighter_ChangeMotionState"],
        );
        match choice.body {
            PortBody::Choice { yes, no, .. } => {
                assert_eq!(
                    yes,
                    PortExpr::Value(PortValue::Action("JumpF".into())),
                );
                assert_eq!(no, PortExpr::Value(PortValue::Action("JumpB".into())));
            }
            _ => panic!("expected a choice"),
        }
    }

    #[test]
    fn emits_rust_retaining_names_provenance_and_typed_inputs() {
        let functions = vec![
            lower_guard(PORT_SOURCE, "ftCo_Turn.c", "ftCo_800C97A8").unwrap(),
            lower_choice(PORT_SOURCE, "ftCo_Jump.c", "ftCo_Jump_Enter").unwrap(),
        ];
        let generated = port_file(&functions);
        assert!(generated.contains("pub fn ftCo_800C97A8("));
        assert!(generated.contains("(query.lstick_x * query.facing_dir) <= common.x34"));
        assert!(generated.contains("pub fn ftCo_Jump_Enter("));
        assert!(generated.contains("if ((query.lstick_x * query.facing_dir) > -common.x78)"));
        assert!(generated.contains("FtMotionId::JumpF"));
        assert!(generated.contains("FtMotionId::JumpB"));
        assert!(generated.contains("0123456789abcdef0123456789abcdef01234567"));
        assert!(generated.contains("ftCo_Turn.c"));
        assert!(generated.contains("Untranslated external calls: ftCommon_8007D5D4, Fighter_ChangeMotionState"));
    }

    #[test]
    fn rejects_expressions_outside_the_translated_subset() {
        let source = "int f(Fighter* fp) { if (fp->input.lstick[0].x + 1) { return 1; } return 0; }\n";
        let error = lower_guard(source, "a.c", "f").unwrap_err();
        assert!(matches!(error, SourceError::UnsupportedExpression(_)));
    }
}
