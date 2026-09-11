use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Zero-based direct-call position within the owning function.
    pub ordinal: usize,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceState {
    pub id: String,
    pub name: String,
    pub ordinal: usize,
    pub source: SourceRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCallRequirement {
    pub id: String,
    pub ordinal: usize,
    pub symbol: String,
    pub operation: Option<String>,
    pub owner: Option<String>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCallback {
    pub id: String,
    pub action: Option<String>,
    pub phase: String,
    pub source: SourceRef,
    pub calls: Vec<SourceCallRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInventoryCounts {
    pub states: usize,
    pub callbacks: usize,
    pub direct_calls: usize,
    pub unresolved: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMachineInventory {
    pub ruleset: String,
    pub repository: String,
    pub revision: String,
    pub vocabulary: SourceRef,
    pub states: Vec<SourceState>,
    pub callbacks: Vec<SourceCallback>,
    pub unresolved: Vec<Unresolved>,
    pub counts: SourceInventoryCounts,
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

/// One value on the generated port's typed boundary. Variants carry neutral
/// tokens (not source spellings); the source expression is recovered for
/// provenance by [`source_expr`].
#[derive(Clone, Debug, PartialEq)]
pub enum PortValue {
    /// A fighter input field, e.g. `fp->input.lstick[0].x`.
    Input(&'static str),
    /// An unresolved tuning limit. The numeric value is not retained, so the
    /// generated Rust exposes it as a typed input field rather than a constant.
    Tuning(&'static str),
    /// A character attribute, exposed as a typed attribute input.
    Attr(&'static str),
    /// A decomp motion id (`ftCo_MS_*`), lowered to a neutral motion.
    Motion(&'static str),
    /// A local variable bound earlier in the translated body.
    Local(&'static str),
    Bool(bool),
    Unsigned(u32),
    Float(f32),
}

/// A pure expression inside the translated subset. Anything outside the subset
/// is rejected with [`SourceError::UnsupportedExpression`].
#[derive(Clone, Debug, PartialEq)]
pub enum PortExpr {
    Value(PortValue),
    Neg(Box<PortExpr>),
    Mul(Box<PortExpr>, Box<PortExpr>),
    Compare(Box<PortExpr>, Op, Box<PortExpr>),
    Conditional {
        condition: Box<PortExpr>,
        yes: Box<PortExpr>,
        no: Box<PortExpr>,
    },
    Vector {
        x: Box<PortExpr>,
        y: Box<PortExpr>,
        z: Box<PortExpr>,
    },
}

/// 1-based line and column extent of a translated node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortSpan {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// One side effect in source order. Variants are neutral mechanics facts; the
/// source call behind each fact is retained in [`PortFn::calls`] and the
/// generated provenance.
#[derive(Clone, Debug, PartialEq)]
pub enum PortEffect {
    /// The per-callback common prelude with no modeled arguments.
    BeginJump,
    /// `Fighter_ChangeMotionState(gobj, msid, flags, anim_start, anim_speed, anim_blend, NULL)`.
    EnterMotion {
        motion: PortExpr,
        flags: &'static str,
        anim_start: f32,
        anim_speed: f32,
        anim_blend: f32,
    },
    /// `ftCo_800CB110(gobj, enabled, scale)`.
    SetJumpParams { enabled: PortExpr, scale: f32 },
    /// `fp->cmd_vars[<slot>] = <value>`.
    SetCommandValue { slot: u32, value: PortExpr },
    /// `fp-><field> = <value>` for a retained boolean flag.
    SetFlag { value: PortExpr },
    /// `ftCo_800CBAC4(gobj, msid, &vel, flag)`.
    LaunchJump {
        motion: PortExpr,
        velocity: &'static str,
        flag: PortExpr,
    },
}

/// A translated statement: a pure local binding or one ordered side effect.
#[derive(Clone, Debug, PartialEq)]
pub enum PortStatement {
    Bind { name: String, expr: PortExpr },
    Effect(PortEffect),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PortBody {
    /// `return <comparison>;`, e.g. `ftCo_800C97A8`.
    Guard(PortExpr),
    /// Ordered side effects and the pure bindings they read, e.g.
    /// `ftCo_Jump_Enter`.
    Callback(Vec<PortStatement>),
}

/// One lowered decomp callback plus the provenance the generated file retains.
#[derive(Clone, Debug, PartialEq)]
pub struct PortFn {
    pub name: String,
    pub path: String,
    pub function_line: usize,
    pub function_end_line: usize,
    /// Span of the decision expression, or the whole body for a callback.
    pub span: PortSpan,
    /// Direct calls in the function, boundary macros removed.
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

/// Neutral token for a fighter input field.
fn input_field(text: &str) -> Option<&'static str> {
    Some(match text {
        "fp->input.lstick[0].x" => "StickX",
        "fp->facing_dir" => "Facing",
        _ => return None,
    })
}

/// Neutral token for an unresolved tuning limit.
fn tuning_field(text: &str) -> Option<&'static str> {
    Some(match text {
        "x34" => "TurnThreshold",
        "x78" => "JumpBackThreshold",
        _ => return None,
    })
}

/// Neutral token for a character attribute.
fn attr_field(text: &str) -> Option<&'static str> {
    Some(match text {
        "air_jump_h_multiplier" => "AirJumpHScale",
        "jump_v_initial_velocity" => "JumpInitialSpeed",
        "air_jump_v_multiplier" => "AirJumpVScale",
        _ => return None,
    })
}

/// Neutral token for a decomp motion id.
fn motion_name(text: &str) -> Option<&'static str> {
    Some(match text {
        "JumpF" => "JumpForward",
        "JumpB" => "JumpBackward",
        "JumpAerialF" => "AirJumpForward",
        "JumpAerialB" => "AirJumpBackward",
        _ => return None,
    })
}

/// Neutral token for a decomp local variable.
fn local_slot(text: &str) -> Option<&'static str> {
    Some(match text {
        "msid" => "Motion",
        "vel" => "Velocity",
        _ => return None,
    })
}

/// Neutral token for a `Ft_MF_*` motion flag.
fn motion_flag(text: &str) -> Option<&'static str> {
    Some(match text {
        "None" => "None",
        _ => return None,
    })
}

/// Neutral rule id for a lowered callback.
fn rule_id(symbol: &str) -> Option<&'static str> {
    Some(match symbol {
        "ftCo_800C97A8" => "TurnRequest",
        "ftCo_Jump_Enter" => "Takeoff",
        "ftCo_JumpAerial_Enter_Basic" => "AirJump",
        _ => return None,
    })
}

fn input_source(token: &str) -> &str {
    match token {
        "StickX" => "fp->input.lstick[0].x",
        "Facing" => "fp->facing_dir",
        other => other,
    }
}

fn tuning_source(token: &str) -> &str {
    match token {
        "TurnThreshold" => "p_ftCommonData->x34",
        "JumpBackThreshold" => "p_ftCommonData->x78",
        other => other,
    }
}

fn attr_source(token: &str) -> &str {
    match token {
        "AirJumpHScale" => "fp->co_attrs.air_jump_h_multiplier",
        "JumpInitialSpeed" => "fp->co_attrs.jump_v_initial_velocity",
        "AirJumpVScale" => "fp->co_attrs.air_jump_v_multiplier",
        other => other,
    }
}

fn motion_source(token: &str) -> &str {
    match token {
        "JumpForward" => "ftCo_MS_JumpF",
        "JumpBackward" => "ftCo_MS_JumpB",
        "AirJumpForward" => "ftCo_MS_JumpAerialF",
        "AirJumpBackward" => "ftCo_MS_JumpAerialB",
        other => other,
    }
}

fn local_source(token: &str) -> &str {
    match token {
        "Motion" => "msid",
        "Velocity" => "vel",
        other => other,
    }
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
        "true" => return Some(PortValue::Bool(true)),
        "false" => return Some(PortValue::Bool(false)),
        _ => {}
    }
    if let Some(field) = canonical_text.strip_prefix("fp->co_attrs.") {
        if let Some(slot) = attr_field(field) {
            return Some(PortValue::Attr(slot));
        }
    }
    if let Some(field) = canonical_text.strip_prefix("co_attrs->") {
        if let Some(slot) = attr_field(field) {
            return Some(PortValue::Attr(slot));
        }
    }
    if let Some(field) = canonical_text.strip_prefix("p_ftCommonData->") {
        if let Some(slot) = tuning_field(field) {
            return Some(PortValue::Tuning(slot));
        }
    }
    if let Some(action) = canonical_text.strip_prefix("ftCo_MS_") {
        if let Some(slot) = motion_name(action) {
            return Some(PortValue::Motion(slot));
        }
    }
    if let Some(slot) = input_field(canonical_text) {
        return Some(PortValue::Input(slot));
    }
    if let Some(slot) = local_slot(canonical_text) {
        return Some(PortValue::Local(slot));
    }
    if canonical_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return canonical_text.parse().ok().map(PortValue::Unsigned);
    }
    None
}

fn float_literal(node: Node<'_>, source: &str) -> Result<f32, SourceError> {
    let value = canonical(node, source);
    let trimmed = value.trim_end_matches(['f', 'F']);
    trimmed
        .parse()
        .map_err(|_| SourceError::UnsupportedExpression(value))
}

fn lower_expr(node: Node<'_>, source: &str) -> Result<PortExpr, SourceError> {
    let node = skip_parens(node, source);
    match node.kind() {
        "number_literal" => {
            let value = canonical(node, source);
            if value.contains('.') || value.contains('e') || value.contains('E') {
                Ok(PortExpr::Value(PortValue::Float(float_literal(node, source)?)))
            } else {
                port_value(&value)
                    .map(PortExpr::Value)
                    .ok_or(SourceError::UnsupportedExpression(value))
            }
        }
        "true" | "false" => Ok(PortExpr::Value(PortValue::Bool(node.kind() == "true"))),
        "pointer_expression" => {
            // `&vel`: the effect carries the vector by value, so drop the address.
            let argument = node
                .child_by_field_name("argument")
                .or_else(|| node.named_child(0))
                .ok_or(SourceError::Parse)?;
            lower_expr(argument, source)
        }
        "conditional_expression" => {
            let condition = node.child_by_field_name("condition").ok_or(SourceError::Parse)?;
            let yes = node.child_by_field_name("consequence").ok_or(SourceError::Parse)?;
            let no = node.child_by_field_name("alternative").ok_or(SourceError::Parse)?;
            Ok(PortExpr::Conditional {
                condition: Box::new(lower_expr(condition, source)?),
                yes: Box::new(lower_expr(yes, source)?),
                no: Box::new(lower_expr(no, source)?),
            })
        }
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

/// Calls that are the typed boundary itself or source padding, not engine work
/// the generated port should emit.
const BOUNDARY_CALLS: &[&str] = &["GET_FIGHTER", "PAD_STACK"];

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

/// Calls the callback lowerer maps to typed effects. Any other call is
/// unsupported and reported.
const KNOWN_EFFECT_CALLS: &[&str] = &[
    "ftCommon_8007D5D4",
    "Fighter_ChangeMotionState",
    "ftCo_800CB110",
    "ftCo_800CBAC4",
];

fn call_arguments(node: Node<'_>) -> Result<Vec<Node<'_>>, SourceError> {
    let arguments = node.child_by_field_name("arguments").ok_or(SourceError::Parse)?;
    let mut cursor = arguments.walk();
    Ok(arguments.named_children(&mut cursor).collect())
}

fn field_parts(node: Node<'_>, source: &str) -> Option<(String, String)> {
    if node.kind() != "field_expression" {
        return None;
    }
    let argument = node.child_by_field_name("argument")?;
    let field = node.child_by_field_name("field")?;
    Some((canonical(argument, source), text(field, source).into()))
}

fn flow_local_name(node: Node<'_>, source: &str) -> String {
    let node = if node.kind() == "pointer_expression" {
        node.named_child(0).unwrap_or(node)
    } else {
        node
    };
    canonical(node, source)
}

fn lower_call(node: Node<'_>, source: &str) -> Result<Option<PortEffect>, SourceError> {
    let callee = node
        .child_by_field_name("function")
        .map(|callee| canonical(callee, source))
        .ok_or(SourceError::Parse)?;
    if BOUNDARY_CALLS.contains(&callee.as_str()) {
        return Ok(None);
    }
    if !KNOWN_EFFECT_CALLS.contains(&callee.as_str()) {
        return Err(SourceError::UnsupportedExpression(callee));
    }
    let arguments = call_arguments(node)?;
    let effect = match callee.as_str() {
        "ftCommon_8007D5D4" => PortEffect::BeginJump,
        "Fighter_ChangeMotionState" => {
            let flags = canonical(*arguments.get(2).ok_or(SourceError::Parse)?, source)
                .strip_prefix("Ft_MF_")
                .and_then(motion_flag)
                .ok_or_else(|| SourceError::UnsupportedExpression(callee.clone()))?;
            PortEffect::EnterMotion {
                motion: lower_expr(*arguments.get(1).ok_or(SourceError::Parse)?, source)?,
                flags,
                anim_start: float_literal(*arguments.get(3).ok_or(SourceError::Parse)?, source)?,
                anim_speed: float_literal(*arguments.get(4).ok_or(SourceError::Parse)?, source)?,
                anim_blend: float_literal(*arguments.get(5).ok_or(SourceError::Parse)?, source)?,
            }
        }
        "ftCo_800CB110" => PortEffect::SetJumpParams {
            enabled: lower_expr(*arguments.get(1).ok_or(SourceError::Parse)?, source)?,
            scale: float_literal(*arguments.get(2).ok_or(SourceError::Parse)?, source)?,
        },
        "ftCo_800CBAC4" => {
            let velocity = flow_local_name(*arguments.get(2).ok_or(SourceError::Parse)?, source);
            PortEffect::LaunchJump {
                motion: lower_expr(*arguments.get(1).ok_or(SourceError::Parse)?, source)?,
                velocity: local_slot(&velocity)
                    .ok_or_else(|| SourceError::UnsupportedExpression(velocity.clone()))?,
                flag: lower_expr(*arguments.get(3).ok_or(SourceError::Parse)?, source)?,
            }
        }
        other => return Err(SourceError::UnsupportedExpression(other.into())),
    };
    Ok(Some(effect))
}

fn vector_component(
    node: Node<'_>,
    source: &str,
) -> Result<Option<(String, usize, PortExpr)>, SourceError> {
    let expression = if node.kind() == "expression_statement" {
        match node.named_child(0) {
            Some(expression) => expression,
            None => return Ok(None),
        }
    } else {
        node
    };
    if expression.kind() != "assignment_expression" {
        return Ok(None);
    }
    let left = expression.child_by_field_name("left").ok_or(SourceError::Parse)?;
    let Some((argument, field)) = field_parts(left, source) else {
        return Ok(None);
    };
    let axis = match field.as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return Ok(None),
    };
    if argument == "fp" || !valid_ident(&argument) {
        return Ok(None);
    }
    let slot = local_slot(&argument)
        .ok_or_else(|| SourceError::UnsupportedExpression(argument.clone()))?;
    let right = expression.child_by_field_name("right").ok_or(SourceError::Parse)?;
    Ok(Some((slot.into(), axis, lower_expr(right, source)?)))
}

fn flush_vector(
    statements: &mut Vec<PortStatement>,
    pending: &mut Option<(String, [Option<PortExpr>; 3])>,
) -> Result<(), SourceError> {
    let Some((name, components)) = pending.take() else {
        return Ok(());
    };
    let [Some(x), Some(y), Some(z)] = components else {
        return Err(SourceError::MissingNode {
            function: name,
            kind: "vector component",
        });
    };
    statements.push(PortStatement::Bind {
        name,
        expr: PortExpr::Vector {
            x: Box::new(x),
            y: Box::new(y),
            z: Box::new(z),
        },
    });
    Ok(())
}

fn lower_statement(
    node: Node<'_>,
    source: &str,
    statements: &mut Vec<PortStatement>,
    pending: &mut Option<(String, [Option<PortExpr>; 3])>,
) -> Result<(), SourceError> {
    match node.kind() {
        "comment" | "declaration" => return Ok(()),
        _ => {}
    }
    if let Some((name, axis, value)) = vector_component(node, source)? {
        if pending.as_ref().is_some_and(|(pending_name, _)| *pending_name != name) {
            flush_vector(statements, pending)?;
        }
        if pending.is_none() {
            *pending = Some((name, [None, None, None]));
        }
        let slot = &mut pending.as_mut().expect("initialized above").1[axis];
        if slot.is_some() {
            return Err(SourceError::UnsupportedExpression(canonical(node, source)));
        }
        *slot = Some(value);
        return Ok(());
    }
    flush_vector(statements, pending)?;

    let expression = if node.kind() == "expression_statement" {
        node.named_child(0).ok_or(SourceError::Parse)?
    } else {
        node
    };
    match expression.kind() {
        "assignment_expression" => {
            let left = expression.child_by_field_name("left").ok_or(SourceError::Parse)?;
            let right = expression.child_by_field_name("right").ok_or(SourceError::Parse)?;
            if left.kind() == "identifier" {
                let target = text(left, source);
                if target == "co_attrs" {
                    // `co_attrs = &fp->co_attrs` becomes the typed `attrs` input.
                    return Ok(());
                }
                if let Some(slot) = local_slot(target) {
                    statements.push(PortStatement::Bind {
                        name: slot.into(),
                        expr: lower_expr(right, source)?,
                    });
                    return Ok(());
                }
            }
            let value = lower_expr(right, source)?;
            if let Some((argument, field)) = field_parts(left, source) {
                if argument == "fp" && field == "x2227_b0" {
                    statements.push(PortStatement::Effect(PortEffect::SetFlag { value }));
                    return Ok(());
                }
            }
            if left.kind() == "subscript_expression" {
                let argument = left
                    .child_by_field_name("argument")
                    .map(|argument| canonical(argument, source))
                    .unwrap_or_default();
                let index = left
                    .child_by_field_name("index")
                    .ok_or(SourceError::Parse)
                    .and_then(|index| {
                        canonical(index, source).parse::<u32>().map_err(|_| {
                            SourceError::UnsupportedExpression(canonical(left, source))
                        })
                    })?;
                if argument == "fp->cmd_vars" {
                    statements.push(PortStatement::Effect(PortEffect::SetCommandValue {
                        slot: index,
                        value,
                    }));
                    return Ok(());
                }
            }
            Err(SourceError::UnsupportedExpression(canonical(expression, source)))
        }
        "call_expression" => {
            if let Some(effect) = lower_call(expression, source)? {
                statements.push(PortStatement::Effect(effect));
            }
            Ok(())
        }
        _ => Err(SourceError::UnsupportedExpression(canonical(expression, source))),
    }
}

/// Lower a callback body into ordered typed statements. Pure bindings (local
/// scalar and `Vec3` assignments) and side effects retain source order; the
/// emitter renders them as fixed-size arrays.
pub fn lower_callback(source: &str, path: &str, name: &str) -> Result<PortFn, SourceError> {
    let tree = tree(source)?;
    let function = function(tree.root_node(), source, name)?;
    let body_node = function.child_by_field_name("body").ok_or(SourceError::Parse)?;
    let mut statements = Vec::new();
    let mut pending = None;
    let mut cursor = body_node.walk();
    for statement in body_node.named_children(&mut cursor) {
        lower_statement(statement, source, &mut statements, &mut pending)?;
    }
    flush_vector(&mut statements, &mut pending)?;
    port_fn(
        source,
        path,
        name,
        function,
        function,
        PortBody::Callback(statements),
    )
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
/// generated provenance keeps the decomp identifiers searchable.
fn source_expr(expr: &PortExpr) -> String {
    match expr {
        PortExpr::Value(PortValue::Input(token)) => input_source(token).into(),
        PortExpr::Value(PortValue::Tuning(token)) => tuning_source(token).into(),
        PortExpr::Value(PortValue::Attr(token)) => attr_source(token).into(),
        PortExpr::Value(PortValue::Motion(token)) => motion_source(token).into(),
        PortExpr::Value(PortValue::Local(token)) => local_source(token).into(),
        PortExpr::Value(PortValue::Bool(value)) => value.to_string(),
        PortExpr::Value(PortValue::Unsigned(value)) => value.to_string(),
        PortExpr::Value(PortValue::Float(value)) => format!("{value:?}F"),
        PortExpr::Neg(inner) => format!("-{}", source_expr(inner)),
        PortExpr::Mul(lhs, rhs) => format!("{}*{}", source_expr(lhs), source_expr(rhs)),
        PortExpr::Compare(lhs, op, rhs) => {
            format!("{} {} {}", source_expr(lhs), op_text(*op), source_expr(rhs))
        }
        PortExpr::Conditional { condition, yes, no } => {
            format!(
                "{} ? {} : {}",
                source_expr(condition),
                source_expr(yes),
                source_expr(no),
            )
        }
        PortExpr::Vector { x, y, z } => format!(
            "{{{}, {}, {}}}",
            source_expr(x),
            source_expr(y),
            source_expr(z),
        ),
    }
}

/// Render a lowered expression as generated neutral Rust data. The result is a
/// reference expression of type `&'static Expr`.
fn emit_data_expr(expr: &PortExpr) -> String {
    match expr {
        PortExpr::Value(PortValue::Input(field)) => format!("&Expr::Input(InputField::{field})"),
        PortExpr::Value(PortValue::Tuning(field)) => {
            format!("&Expr::Tuning(TuningField::{field})")
        }
        PortExpr::Value(PortValue::Attr(field)) => format!("&Expr::Attr(AttrField::{field})"),
        PortExpr::Value(PortValue::Motion(name)) => format!("&Expr::Motion(Motion::{name})"),
        PortExpr::Value(PortValue::Local(slot)) => format!("&Expr::Local(LocalSlot::{slot})"),
        PortExpr::Value(PortValue::Bool(value)) => format!("&Expr::Flag({value})"),
        PortExpr::Value(PortValue::Unsigned(value)) => format!("&Expr::Count({value})"),
        PortExpr::Value(PortValue::Float(value)) => format!("&Expr::Number({value:?})"),
        PortExpr::Neg(inner) => format!("&Expr::Neg({})", emit_data_expr(inner)),
        PortExpr::Mul(lhs, rhs) => {
            format!("&Expr::Mul({}, {})", emit_data_expr(lhs), emit_data_expr(rhs))
        }
        PortExpr::Compare(lhs, op, rhs) => format!(
            "&Expr::Compare({}, CompareOp::{}, {})",
            emit_data_expr(lhs),
            op_variant(*op),
            emit_data_expr(rhs),
        ),
        PortExpr::Conditional { condition, yes, no } => format!(
            "&Expr::Select {{ condition: {}, yes: {}, no: {} }}",
            emit_data_expr(condition),
            emit_data_expr(yes),
            emit_data_expr(no),
        ),
        PortExpr::Vector { x, y, z } => format!(
            "&Expr::Vector {{ x: {}, y: {}, z: {} }}",
            emit_data_expr(x),
            emit_data_expr(y),
            emit_data_expr(z),
        ),
    }
}

fn op_variant(op: Op) -> &'static str {
    match op {
        Op::Less => "Less",
        Op::LessEqual => "LessEqual",
        Op::Greater => "Greater",
        Op::GreaterEqual => "GreaterEqual",
        Op::Equal => "Equal",
        Op::NotEqual => "NotEqual",
    }
}

fn emit_data_effect(effect: &PortEffect) -> String {
    match effect {
        PortEffect::BeginJump => "Effect::BeginJump".into(),
        PortEffect::EnterMotion {
            motion,
            flags,
            anim_start,
            anim_speed,
            anim_blend,
        } => format!(
            "Effect::EnterMotion {{ motion: {}, flags: MotionFlags::{flags}, \
             anim_start: {anim_start:?}, anim_speed: {anim_speed:?}, anim_blend: {anim_blend:?} }}",
            emit_data_expr(motion),
        ),
        PortEffect::SetJumpParams { enabled, scale } => format!(
            "Effect::SetJumpParams {{ enabled: {}, scale: {scale:?} }}",
            emit_data_expr(enabled),
        ),
        PortEffect::SetCommandValue { slot, value } => format!(
            "Effect::SetCommandValue {{ slot: {slot}, value: {} }}",
            emit_data_expr(value),
        ),
        PortEffect::SetFlag { value } => {
            format!("Effect::SetFlag {{ value: {} }}", emit_data_expr(value))
        }
        PortEffect::LaunchJump {
            motion,
            velocity,
            flag,
        } => format!(
            "Effect::LaunchJump {{ motion: {}, velocity: LocalSlot::{velocity}, flag: {} }}",
            emit_data_expr(motion),
            emit_data_expr(flag),
        ),
    }
}

/// Escape source expression text for a Rust string literal, so provenance
/// strings survive round trips through the generated module.
fn escape_source(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}


/// Render lowered callbacks as generated neutral Rust rule data. Runtime
/// declarations are neutral; the original source symbol, line span, call list
/// and source expression text travel under [`RuleProvenance`]. Repository
/// identity stays in the offline artifacts rather than the runtime-facing
/// module. No source value is invented.
pub fn emit_port_rust(file: &PortFile<'_>) -> Result<String, SourceError> {
    let mut output = String::from(
        "// @generated by smash-import from pinned source evidence.\n\
         // Do not edit by hand; run `just source-rules`.\n",
    );
    output.push_str(&format!("// source revision: {}\n\n", file.revision));
    output.push_str("#![allow(unused_imports)]\n\n");
    output.push_str(
        "use crate::{\n    AttrField, CompareOp, Effect, Expr, InputField, LocalSlot, Motion, MotionFlags, Rule,\n    RuleId, RuleProvenance, Statement, TuningField,\n};\n\n",
    );
    output.push_str("pub static RULES: &[Rule] = &[\n");
    for function in file.functions {
        let id = rule_id(&function.name)
            .ok_or_else(|| SourceError::UnsupportedExpression(function.name.clone()))?;
        output.push_str("    Rule {\n");
        output.push_str(&format!("        id: RuleId::{id},\n"));
        match &function.body {
            PortBody::Guard(expr) => {
                output.push_str(&format!("        guard: Some({}),\n", emit_data_expr(expr)));
                output.push_str("        statements: &[],\n");
            }
            PortBody::Callback(statements) => {
                output.push_str("        guard: None,\n");
                output.push_str("        statements: &[\n");
                for statement in statements {
                    match statement {
                        PortStatement::Bind { name, expr } => output.push_str(&format!(
                            "            Statement::Bind {{ slot: LocalSlot::{name}, value: {} }},\n",
                            emit_data_expr(expr),
                        )),
                        PortStatement::Effect(effect) => output.push_str(&format!(
                            "            Statement::Emit(&{}),\n",
                            emit_data_effect(effect),
                        )),
                    }
                }
                output.push_str("        ],\n");
            }
        }
        let calls = function
            .calls
            .iter()
            .map(|call| format!("\"{}\"", escape_source(call)))
            .collect::<Vec<_>>()
            .join(", ");
        let (source_guard, source_bindings): (String, Vec<String>) = match &function.body {
            PortBody::Guard(expr) => (source_expr(expr), Vec::new()),
            PortBody::Callback(statements) => (
                String::new(),
                statements
                    .iter()
                    .filter_map(|statement| match statement {
                        PortStatement::Bind { name, expr } => Some(format!(
                            "{} = {}",
                            local_source(name),
                            source_expr(expr),
                        )),
                        PortStatement::Effect(_) => None,
                    })
                    .collect(),
            ),
        };
        let bindings = source_bindings
            .iter()
            .map(|binding| format!("\"{}\"", escape_source(binding)))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str("        provenance: RuleProvenance {\n");
        output.push_str(&format!(
            "            source_symbol: \"{}\",\n",
            escape_source(&function.name),
        ));
        output.push_str(&format!(
            "            function_lines: ({}, {}),\n",
            function.function_line, function.function_end_line,
        ));
        output.push_str(&format!(
            "            span: ({}, {}, {}, {}),\n",
            function.span.line,
            function.span.column,
            function.span.end_line,
            function.span.end_column,
        ));
        output.push_str(&format!("            calls: &[{calls}],\n"));
        output.push_str(&format!(
            "            source_guard: \"{}\",\n",
            escape_source(&source_guard),
        ));
        output.push_str(&format!("            source_bindings: &[{bindings}],\n"));
        output.push_str("        },\n");
        output.push_str("    },\n");
    }
    output.push_str("];\n");
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
    ordinals: &mut BTreeMap<usize, usize>,
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
            ordinal: {
                let ordinal = ordinals.entry(function).or_default();
                let result = *ordinal;
                *ordinal += 1;
                result
            },
            operation: None,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, source, file, current, functions, calls, ordinals);
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
    let mut ordinals = BTreeMap::new();
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
            &mut ordinals,
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

fn enum_node<'a>(root: Node<'a>, source: &str, name: &str) -> Result<Node<'a>, SourceError> {
    let mut found = None;
    visit(root, &mut |node| {
        if found.is_some() || node.kind() != "enum_specifier" {
            return;
        }
        if node
            .child_by_field_name("name")
            .is_some_and(|enum_name| text(enum_name, source) == name)
        {
            found = Some(node);
        }
    });
    found.ok_or_else(|| SourceError::MissingFunction(name.into()))
}

/// Extract the ordered members of a named C enum from a tree-sitter syntax
/// tree. Sentinel members are retained in the parse but omitted from the
/// action denominator by name, without evaluating C enum expressions.
pub fn motion_states(
    ruleset: &str,
    source: &str,
    repository: &str,
    revision: &str,
    path: &str,
    enum_name: &str,
) -> Result<Vec<SourceState>, SourceError> {
    let tree = tree(source)?;
    let enumeration = enum_node(tree.root_node(), source, enum_name)?;
    let body = enumeration
        .child_by_field_name("body")
        .ok_or(SourceError::MissingNode {
            function: enum_name.into(),
            kind: "enum body",
        })?;
    let mut states = Vec::new();
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "enumerator" {
            continue;
        }
        let member_name = member
            .child_by_field_name("name")
            .map(|name| text(name, source).to_string())
            .ok_or(SourceError::Parse)?;
        if member_name == "ftCo_MS_None" || member_name == "ftCo_MS_Count" {
            continue;
        }
        let ordinal = states.len();
        states.push(SourceState {
            id: format!(
                "{ruleset}:{revision}:{path}:{enum_name}:{member_name}:{ordinal}"
            ),
            name: member_name
                .strip_prefix("ftCo_MS_")
                .unwrap_or(&member_name)
                .to_string(),
            ordinal,
            source: SourceRef {
                repository: repository.into(),
                revision: revision.into(),
                path: path.into(),
                line: member.start_position().row + 1,
                symbol: member_name,
            },
        });
    }
    Ok(states)
}

fn callback_phase(name: &str) -> Option<(&'static str, &str)> {
    for phase in ["Anim", "IASA", "Phys", "Coll"] {
        let suffix = format!("_{phase}");
        if let Some(prefix) = name.strip_suffix(&suffix) {
            return Some((phase, prefix));
        }
        let suffix = format!("_{phase}_Inner");
        if let Some(prefix) = name.strip_suffix(&suffix) {
            return Some((phase, prefix));
        }
    }
    None
}

fn source_callback_id(
    ruleset: &str,
    revision: &str,
    path: &str,
    symbol: &str,
    phase: &str,
    ordinal: usize,
) -> String {
    format!("{ruleset}:{revision}:{path}:{symbol}:{phase}:{ordinal}")
}

/// Build the source-side state and callback denominator. The enum supplies
/// state membership; [`common_inventory`] supplies every parsed function and
/// direct call. Callback records retain source order and unsupported calls as
/// unresolved facts. No transition destination is inferred.
pub fn source_machine_inventory(
    ruleset: &str,
    repository: &str,
    revision: &str,
    vocabulary_path: &str,
    vocabulary_source: &str,
    sources: &[(String, String)],
) -> Result<SourceMachineInventory, SourceError> {
    let states = motion_states(
        ruleset,
        vocabulary_source,
        repository,
        revision,
        vocabulary_path,
        "ftCommon_MotionState",
    )?;
    let inventory = common_inventory(sources)?;
    let state_names: std::collections::BTreeSet<&str> =
        states.iter().map(|state| state.name.as_str()).collect();
    let mut unresolved = Vec::new();
    for file in &inventory.parse_errors {
        unresolved.push(Unresolved {
            symbol: inventory.files[*file].clone(),
            source: SourceRef {
                repository: repository.into(),
                revision: revision.into(),
                path: inventory.files[*file].clone(),
                line: 0,
                symbol: inventory.files[*file].clone(),
            },
            reason: "tree-sitter recovered a syntax-error file; recovered records remain visible".into(),
        });
    }

    let mut calls_by_function: BTreeMap<usize, Vec<&CallSite>> = BTreeMap::new();
    for call in &inventory.calls {
        calls_by_function.entry(call.function).or_default().push(call);
    }
    for calls in calls_by_function.values_mut() {
        calls.sort_by_key(|call| call.ordinal);
    }
    let mut callbacks = Vec::new();
    for (function_index, function) in inventory.functions.iter().enumerate() {
        let Some((phase, prefix)) = callback_phase(&function.name) else {
            continue;
        };
        let path = &inventory.files[function.file];
        let action = prefix
            .strip_prefix("ftCo_")
            .filter(|name| state_names.contains(*name))
            .map(str::to_string);
        let callback_ordinal = callbacks.len();
        let callback_id = source_callback_id(
            ruleset,
            revision,
            path,
            &function.name,
            phase,
            callback_ordinal,
        );
        let source = SourceRef {
            repository: repository.into(),
            revision: revision.into(),
            path: path.clone(),
            line: function.line,
            symbol: function.name.clone(),
        };
        if action.is_none() {
            unresolved.push(Unresolved {
                symbol: function.name.clone(),
                source: source.clone(),
                reason: "callback prefix has no exact ftCommon motion-state member".into(),
            });
        }
        let mut callback_calls = Vec::new();
        for call in calls_by_function.get(&function_index).into_iter().flatten() {
            let (operation, owner) = call
                .operation
                .map(|operation| {
                    (
                        Some(RECOGNIZED_OPERATIONS[operation].1.to_string()),
                        Some(RECOGNIZED_OPERATIONS[operation].2.to_string()),
                    )
                })
                .unwrap_or((None, None));
            let call_source = SourceRef {
                repository: repository.into(),
                revision: revision.into(),
                path: path.clone(),
                line: call.line,
                symbol: call.symbol.clone(),
            };
            let call_id = format!("{callback_id}:call:{}", call.ordinal);
            if operation.is_none() {
                unresolved.push(Unresolved {
                    symbol: call.symbol.clone(),
                    source: call_source.clone(),
                    reason: "direct callee is outside the recognized operation boundary".into(),
                });
            }
            callback_calls.push(SourceCallRequirement {
                id: call_id,
                ordinal: call.ordinal,
                symbol: call.symbol.clone(),
                operation,
                owner,
                source: call_source,
            });
        }
        callbacks.push(SourceCallback {
            id: callback_id,
            action,
            phase: phase.into(),
            source,
            calls: callback_calls,
        });
    }
    let direct_calls = callbacks.iter().map(|callback| callback.calls.len()).sum();
    let counts = SourceInventoryCounts {
        states: states.len(),
        callbacks: callbacks.len(),
        direct_calls,
        unresolved: unresolved.len(),
    };
    Ok(SourceMachineInventory {
        ruleset: ruleset.into(),
        repository: repository.into(),
        revision: revision.into(),
        vocabulary: SourceRef {
            repository: repository.into(),
            revision: revision.into(),
            path: vocabulary_path.into(),
            line: 1,
            symbol: "ftCommon_MotionState".into(),
        },
        states,
        callbacks,
        unresolved,
        counts,
    })
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
    Fighter_ChangeMotionState(gobj, msid, Ft_MF_None, 0.0F, 1.0F, 0.0F, NULL);
    ftCo_800CB110(gobj, true, 1.0F);
    fp->x2227_b0 = true;
}

void ftCo_JumpAerial_Enter_Basic(Fighter_GObj* gobj) {
    ftCo_DatAttrs* co_attrs;
    Fighter* fp = GET_FIGHTER(gobj);
    FtMotionId msid;
    Vec3 vel;
    co_attrs = &fp->co_attrs;
    PAD_STACK(8);

    ftCommon_8007D5D4(fp);
    fp->cmd_vars[0] = 1;
    msid = (fp->input.lstick[0].x * fp->facing_dir) > -p_ftCommonData->x78
               ? ftCo_MS_JumpAerialF
               : ftCo_MS_JumpAerialB;
    vel.x = fp->input.lstick[0].x * co_attrs->air_jump_h_multiplier;
    vel.y =
        co_attrs->jump_v_initial_velocity * co_attrs->air_jump_v_multiplier;
    vel.z = 0.0F;
    ftCo_800CBAC4(gobj, msid, &vel, true);
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

    fn effect_names(function: &PortFn) -> Vec<String> {
        let PortBody::Callback(statements) = &function.body else {
            panic!("expected a callback body");
        };
        statements
            .iter()
            .filter_map(|statement| match statement {
                PortStatement::Bind { .. } => None,
                PortStatement::Effect(effect) => Some(
                    match effect {
                        PortEffect::BeginJump => "BeginJump",
                        PortEffect::EnterMotion { .. } => "EnterMotion",
                        PortEffect::SetJumpParams { .. } => "SetJumpParams",
                        PortEffect::SetCommandValue { .. } => "SetCommandValue",
                        PortEffect::SetFlag { .. } => "SetFlag",
                        PortEffect::LaunchJump { .. } => "LaunchJump",
                    }
                    .into(),
                ),
            })
            .collect()
    }

    #[test]
    fn lowers_guard_to_a_pure_boolean_expression() {
        let guard = lower_guard(PORT_SOURCE, "ftCo_Turn.c", "ftCo_800C97A8").unwrap();
        assert_eq!(guard.calls, Vec::<String>::new());
        assert_eq!(
            guard.body,
            PortBody::Guard(PortExpr::Compare(
                Box::new(PortExpr::Mul(
                    Box::new(PortExpr::Value(PortValue::Input("StickX"))),
                    Box::new(PortExpr::Value(PortValue::Input("Facing"))),
                )),
                Op::LessEqual,
                Box::new(PortExpr::Value(PortValue::Tuning("TurnThreshold"))),
            )),
        );
    }

    #[test]
    fn lowers_callbacks_to_ordered_effects_in_source_order() {
        let jump = lower_callback(PORT_SOURCE, "ftCo_Jump.c", "ftCo_Jump_Enter").unwrap();
        assert_eq!(
            jump.calls,
            ["ftCommon_8007D5D4", "Fighter_ChangeMotionState", "ftCo_800CB110"],
        );
        assert_eq!(
            effect_names(&jump),
            ["BeginJump", "EnterMotion", "SetJumpParams", "SetFlag"],
        );

        let aerial =
            lower_callback(PORT_SOURCE, "ftCo_JumpAerial.c", "ftCo_JumpAerial_Enter_Basic")
                .unwrap();
        assert_eq!(aerial.calls, ["ftCommon_8007D5D4", "ftCo_800CBAC4"]);
        assert_eq!(
            effect_names(&aerial),
            ["BeginJump", "SetCommandValue", "LaunchJump"],
        );
        let PortBody::Callback(statements) = &aerial.body else { panic!() };
        assert!(statements.iter().any(|statement| matches!(
            statement,
            PortStatement::Bind { name, expr: PortExpr::Vector { .. } } if name == "Velocity",
        )));
    }

    #[test]
    fn emits_neutral_rule_data_with_source_provenance_retained() {
        let functions = vec![
            lower_guard(PORT_SOURCE, "ftCo_Turn.c", "ftCo_800C97A8").unwrap(),
            lower_callback(PORT_SOURCE, "ftCo_Jump.c", "ftCo_Jump_Enter").unwrap(),
            lower_callback(PORT_SOURCE, "ftCo_JumpAerial.c", "ftCo_JumpAerial_Enter_Basic")
                .unwrap(),
        ];
        let generated = port_file(&functions);
        assert_eq!(port_file(&functions), generated, "emission must be deterministic");
        // Neutral runtime declarations.
        assert!(generated.contains("pub static RULES: &[Rule] = &["));
        assert!(!generated.contains("pub fn ftCo_"));
        assert!(!generated.contains("FtCommonEffect"));
        assert!(generated.contains("id: RuleId::TurnRequest"));
        assert!(generated.contains("id: RuleId::Takeoff"));
        assert!(generated.contains("id: RuleId::AirJump"));
        assert!(generated.contains(
            "guard: Some(&Expr::Compare(&Expr::Mul(&Expr::Input(InputField::StickX), \
             &Expr::Input(InputField::Facing)), CompareOp::LessEqual, \
             &Expr::Tuning(TuningField::TurnThreshold))),",
        ));
        assert!(generated.contains("&Expr::Tuning(TuningField::JumpBackThreshold)"));
        assert!(generated.contains("Effect::BeginJump"));
        assert!(generated.contains(
            "Effect::EnterMotion { motion: &Expr::Local(LocalSlot::Motion), \
             flags: MotionFlags::None, anim_start: 0.0, anim_speed: 1.0, anim_blend: 0.0 }",
        ));
        assert!(generated
            .contains("Effect::SetJumpParams { enabled: &Expr::Flag(true), scale: 1.0 }"));
        assert!(generated.contains("Effect::SetFlag { value: &Expr::Flag(true) }"));
        assert!(generated
            .contains("Effect::SetCommandValue { slot: 0, value: &Expr::Count(1) }"));
        assert!(generated.contains(
            "Effect::LaunchJump { motion: &Expr::Local(LocalSlot::Motion), \
             velocity: LocalSlot::Velocity, flag: &Expr::Flag(true) }",
        ));
        assert!(generated.contains(
            "&Expr::Vector { x: &Expr::Mul(&Expr::Input(InputField::StickX), \
             &Expr::Attr(AttrField::AirJumpHScale)), \
             y: &Expr::Mul(&Expr::Attr(AttrField::JumpInitialSpeed), \
             &Expr::Attr(AttrField::AirJumpVScale)), z: &Expr::Number(0.0) }",
        ));
        assert!(generated.contains("&Expr::Motion(Motion::JumpForward)"));
        assert!(generated.contains("&Expr::Motion(Motion::AirJumpBackward)"));
        assert!(generated.contains("0123456789abcdef0123456789abcdef01234567"));
        assert!(!generated.contains("github.com"));
        assert!(!generated.contains("ftCo_Turn.c"));
        // Original source identities survive under provenance, not runtime names.
        for symbol in [
            "ftCo_800C97A8",
            "ftCo_Jump_Enter",
            "ftCo_JumpAerial_Enter_Basic",
            "ftCommon_8007D5D4",
            "Fighter_ChangeMotionState",
            "ftCo_800CB110",
            "ftCo_800CBAC4",
            "p_ftCommonData->x34",
            "p_ftCommonData->x78",
            "fp->input.lstick[0].x",
            "fp->facing_dir",
            "ftCo_MS_JumpF",
            "msid = ",
            "vel = ",
        ] {
            assert!(generated.contains(symbol), "missing provenance {symbol}");
        }
        assert!(generated
            .contains("source_bindings: &[\"msid = fp->input.lstick[0].x*fp->facing_dir > \
                     -p_ftCommonData->x78 ? ftCo_MS_JumpF : ftCo_MS_JumpB\"]"));
    }

    #[test]
    fn rejects_expressions_outside_the_translated_subset() {
        let source = "int f(Fighter* fp) { if (fp->input.lstick[0].x + 1) { return 1; } return 0; }\n";
        let error = lower_guard(source, "a.c", "f").unwrap_err();
        assert!(matches!(error, SourceError::UnsupportedExpression(_)));
    }

    const VOCABULARY: &str = r#"
typedef enum ftCommon_MotionState {
    ftCo_MS_None = -1,
    ftCo_MS_Wait,
    ftCo_MS_WalkSlow,
    ftCo_MS_Count,
} ftCommon_MotionState;
"#;

    fn machine(sources: &[(&str, &str)], vocabulary: &str) -> SourceMachineInventory {
        source_machine_inventory(
            "test-ruleset",
            "https://example.invalid/melee.git",
            "0123456789abcdef0123456789abcdef01234567",
            "forward.h",
            vocabulary,
            &sources
                .iter()
                .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn source_machine_regeneration_is_stable() {
        let input = [("ftCo_Wait.c", "void ftCo_Wait_Anim(void) { first(); second(); }\n")];
        assert_eq!(machine(&input, VOCABULARY), machine(&input, VOCABULARY));
    }

    #[test]
    fn source_machine_denominator_grows_with_enum_members() {
        let input = [("ftCo_Wait.c", "void ftCo_Wait_Anim(void) { first(); }\n")];
        let baseline = machine(&input, VOCABULARY);
        let expanded = machine(
            &input,
            &VOCABULARY.replace("ftCo_MS_Count", "ftCo_MS_Run,\n    ftCo_MS_Count"),
        );
        assert_eq!(expanded.states.len(), baseline.states.len() + 1);
        assert_eq!(expanded.states.last().unwrap().name, "Run");
    }

    #[test]
    fn source_machine_preserves_callback_order_and_unresolved_rows() {
        let input = [(
            "ftCo_Wait.c",
            "void ftCo_Wait_Anim(void) { first(); second(); }\n",
        )];
        let inventory = machine(&input, VOCABULARY);
        let callback = inventory
            .callbacks
            .iter()
            .find(|callback| callback.source.symbol == "ftCo_Wait_Anim")
            .unwrap();
        assert_eq!(
            callback.calls.iter().map(|call| call.symbol.as_str()).collect::<Vec<_>>(),
            ["first", "second"],
        );
        assert_eq!(callback.calls.iter().map(|call| call.ordinal).collect::<Vec<_>>(), [0, 1]);
        assert!(inventory.unresolved.iter().any(|row| row.symbol == "first"));
        assert!(inventory.unresolved.iter().any(|row| row.symbol == "second"));
    }

    #[test]
    fn source_machine_retains_parse_error_rows() {
        let inventory = machine(&[("broken.c", "void ftCo_Wait_Anim(void) { first( }\n")], VOCABULARY);
        assert!(inventory.unresolved.iter().any(|row| row.symbol == "broken.c"));
    }
}
