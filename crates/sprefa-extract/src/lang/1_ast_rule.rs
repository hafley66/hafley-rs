//! Typed ast-grep rule requests. `cst {}` remains the Tree-sitter-query surface.
//!
//! YAML decodes into [`AstRuleRequest`], the only rule model. Accepted YAML has
//! `id`, `rule`, optional `utils`, and optional `fix`; callers must reduce
//! official ast-grep files carrying `language`, `severity`, `message`, `files`,
//! or `constraints` before this boundary.

use std::collections::BTreeMap;

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, NodeMatch};
use serde::Serialize;

use crate::lang::extract_lang::RyiLang;
use crate::shape::{content_id_of, ContentId, Span};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AstRule {
    Pattern(String),
    Kind(String),
    Regex(String),
    Matches(String),
    All(Vec<AstRule>),
    Any(Vec<AstRule>),
    Not(Box<AstRule>),
    Inside {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Has {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Follows {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Precedes {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    /// The node's index among its siblings. `position` is 1-based or a CSS
    /// `An+B` form; `of_rule` counts only siblings matching it; `reverse`
    /// counts from the last sibling.
    NthChild {
        position: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        of_rule: Option<Box<AstRule>>,
        reverse: bool,
    },
    /// The node starts and ends at exactly these zero-based `(line, column)`
    /// points.
    Range { start: (u32, u32), end: (u32, u32) },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StopBy {
    End(String),
    Rule(Box<AstRule>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleRequest {
    pub id: String,
    pub rule: AstRule,
    #[serde(default)]
    pub utils: Vec<NamedAstRule>,
    /// A rule each `$NAME` metavariable of a `pattern` must satisfy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<NamedAstRule>,
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NamedAstRule {
    pub id: String,
    pub rule: AstRule,
}

/// Match identity is content identity plus a half-open byte span. `path` is
/// display/routing data and never match identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMatch {
    pub record: &'static str,
    pub query: String,
    pub path: String,
    pub content: ContentId,
    pub span: Span,
    pub captures: Vec<AstRuleCapture>,
    pub proposal: Option<AstRuleMutationProposal>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct AstRuleCapture {
    pub name: String,
    pub text: String,
    pub span: Span,
}

/// A replacement proposal carries the source content identity and byte span.
/// The engine supplies the target `ActionSource` at the Soopy staging seam.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMutationProposal {
    pub query: String,
    pub content: ContentId,
    pub span: Span,
    pub replacement: String,
}

impl AstRuleMutationProposal {
    /// Combines deterministic edits for one source into Soopy's one-action
    /// transaction. `stage_mutations` remains the conflict authority.
    pub fn stage_request_batch(
        proposals: &[Self],
        root: soopy::SourceRootId,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> Result<soopy::StageRequest, &'static str> {
        let Some(first) = proposals.first() else {
            return Err("ast-rule stage request requires at least one proposal");
        };
        if proposals
            .iter()
            .any(|proposal| proposal.content != first.content)
        {
            return Err("ast-rule proposals for one source must share content identity");
        }
        let mut proposals = proposals.to_vec();
        proposals.sort_by(|left, right| {
            (&left.span, &left.query, &left.replacement).cmp(&(
                &right.span,
                &right.query,
                &right.replacement,
            ))
        });
        let edits = proposals
            .into_iter()
            .map(|proposal| soopy::TextEdit {
                range: soopy::ActionSpan {
                    source: source.clone(),
                    start: proposal.span.start.into(),
                    end: proposal.span.end().into(),
                },
                replacement: proposal.replacement.into_bytes(),
                producer: producer.clone().with_rule(proposal.query),
            })
            .collect();
        Ok(soopy::StageRequest::new(
            root,
            vec![soopy::SourceAction::Replace {
                source,
                expected: first.content.clone(),
                edits,
            }],
        ))
    }

    pub fn stage_request(
        &self,
        root: soopy::SourceRootId,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> soopy::StageRequest {
        let edit = soopy::TextEdit {
            range: soopy::ActionSpan {
                source: source.clone(),
                start: self.span.start.into(),
                end: self.span.end().into(),
            },
            replacement: self.replacement.as_bytes().to_vec(),
            producer: producer.with_rule(self.query.clone()),
        };
        soopy::StageRequest::new(
            root,
            vec![soopy::SourceAction::Replace {
                source,
                expected: self.content.clone(),
                edits: vec![edit],
            }],
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AstRuleError {
    NoGrammar(String),
    /// A `kind:` string the target grammar does not spell. ast-grep matches
    /// nothing for one, silently, so it is refused before the run.
    UnknownKind { kind: String, language: String },
    Utf8(String),
    Yaml(String),
    InvalidRule(String),
}

impl std::fmt::Display for AstRuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for AstRuleError {}

pub fn decode_ast_rule_yaml(yaml: &str) -> Result<AstRuleRequest, AstRuleError> {
    let value =
        serde_yaml::from_str(yaml).map_err(|error| AstRuleError::Yaml(error.to_string()))?;
    decode_request_value(value).map_err(AstRuleError::Yaml)
}

fn decode_request_value(value: serde_yaml::Value) -> Result<AstRuleRequest, String> {
    let mut map = yaml_map(value)?;
    let id = yaml_string(take_yaml(&mut map, "id")?)?;
    let rule = decode_rule(take_yaml(&mut map, "rule")?)?;
    let fix = match map.remove(&serde_yaml::Value::String("fix".into())) {
        Some(value) => Some(yaml_string(value)?),
        None => None,
    };
    let utils = match map.remove(&serde_yaml::Value::String("utils".into())) {
        Some(serde_yaml::Value::Mapping(utils)) => utils
            .into_iter()
            .map(|(id, rule)| {
                Ok(NamedAstRule {
                    id: yaml_string(id)?,
                    rule: decode_rule(rule)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        Some(_) => return Err("utils must be a mapping of name to rule".into()),
        None => Vec::new(),
    };
    let constraints = match map.remove(&serde_yaml::Value::String("constraints".into())) {
        Some(serde_yaml::Value::Mapping(constraints)) => constraints
            .into_iter()
            .map(|(id, rule)| {
                Ok(NamedAstRule {
                    id: yaml_string(id)?,
                    rule: decode_rule(rule)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        Some(_) => return Err("constraints must be a mapping of metavariable to rule".into()),
        None => Vec::new(),
    };
    if let Some((field, _)) = map.into_iter().next() {
        return Err(format!(
            "unsupported ast-rule YAML field {}",
            yaml_string(field)?
        ));
    }
    Ok(AstRuleRequest {
        id,
        rule,
        utils,
        constraints,
        fix,
    })
}

fn decode_rule(value: serde_yaml::Value) -> Result<AstRule, String> {
    let map = yaml_map(value)?;
    if map.len() != 1 {
        return Err("a rule must have exactly one operator".into());
    }
    let (operator, value) = map.into_iter().next().expect("len checked");
    let operator = yaml_string(operator)?;
    match operator.as_str() {
        "pattern" => Ok(AstRule::Pattern(yaml_string(value)?)),
        "kind" => Ok(AstRule::Kind(yaml_string(value)?)),
        "regex" => Ok(AstRule::Regex(yaml_string(value)?)),
        "matches" => Ok(AstRule::Matches(yaml_string(value)?)),
        "all" => Ok(AstRule::All(yaml_rules(value)?)),
        "any" => Ok(AstRule::Any(yaml_rules(value)?)),
        "not" => Ok(AstRule::Not(Box::new(decode_rule(value)?))),
        "inside" => relation_rule(value, |rule, stop_by| AstRule::Inside { rule, stop_by }),
        "has" => relation_rule(value, |rule, stop_by| AstRule::Has { rule, stop_by }),
        "follows" => relation_rule(value, |rule, stop_by| AstRule::Follows { rule, stop_by }),
        "precedes" => relation_rule(value, |rule, stop_by| AstRule::Precedes { rule, stop_by }),
        "nthChild" => nth_child_rule(value),
        "range" => range_rule(value),
        _ => Err(format!("unknown ast-rule operator {operator}")),
    }
}

/// `nthChild: 2`, `nthChild: "2n+1"`, or the object form with `ofRule` and
/// `reverse`.
fn nth_child_rule(value: serde_yaml::Value) -> Result<AstRule, String> {
    let mut map = match value {
        serde_yaml::Value::Mapping(map) => map,
        other => {
            return Ok(AstRule::NthChild {
                position: yaml_position(other)?,
                of_rule: None,
                reverse: false,
            })
        }
    };
    let position = yaml_position(take_yaml(&mut map, "position")?)?;
    let of_rule = match map.remove(&serde_yaml::Value::String("ofRule".into())) {
        Some(rule) => Some(Box::new(decode_rule(rule)?)),
        None => None,
    };
    let reverse = match map.remove(&serde_yaml::Value::String("reverse".into())) {
        Some(serde_yaml::Value::Bool(reverse)) => reverse,
        Some(_) => return Err("nthChild reverse must be a boolean".into()),
        None => false,
    };
    Ok(AstRule::NthChild {
        position,
        of_rule,
        reverse,
    })
}

fn yaml_position(value: serde_yaml::Value) -> Result<String, String> {
    match value {
        serde_yaml::Value::Number(number) => Ok(number.to_string()),
        serde_yaml::Value::String(text) => Ok(text),
        _ => Err("nthChild position must be a number or An+B string".into()),
    }
}

/// `range: {start: {line, column}, end: {line, column}}`, zero-based.
fn range_rule(value: serde_yaml::Value) -> Result<AstRule, String> {
    let mut map = yaml_map(value)?;
    let start = yaml_point(take_yaml(&mut map, "start")?)?;
    let end = yaml_point(take_yaml(&mut map, "end")?)?;
    Ok(AstRule::Range { start, end })
}

fn yaml_point(value: serde_yaml::Value) -> Result<(u32, u32), String> {
    let mut map = yaml_map(value)?;
    let number = |value: serde_yaml::Value| match value {
        serde_yaml::Value::Number(number) => number
            .as_u64()
            .map(|number| number as u32)
            .ok_or_else(|| "range line and column are unsigned".to_string()),
        _ => Err("range line and column are numbers".into()),
    };
    Ok((
        number(take_yaml(&mut map, "line")?)?,
        number(take_yaml(&mut map, "column")?)?,
    ))
}

fn relation_rule(
    value: serde_yaml::Value,
    constructor: impl FnOnce(Box<AstRule>, Option<StopBy>) -> AstRule,
) -> Result<AstRule, String> {
    let mut map = yaml_map(value)?;
    let stop_by = match map.remove(&serde_yaml::Value::String("stopBy".into())) {
        Some(serde_yaml::Value::String(value)) => Some(StopBy::End(value)),
        Some(value) => Some(StopBy::Rule(Box::new(decode_rule(value)?))),
        None => None,
    };
    Ok(constructor(
        Box::new(decode_rule(serde_yaml::Value::Mapping(map))?),
        stop_by,
    ))
}

fn yaml_rules(value: serde_yaml::Value) -> Result<Vec<AstRule>, String> {
    match value {
        serde_yaml::Value::Sequence(values) => values.into_iter().map(decode_rule).collect(),
        _ => Err("rule list must be a YAML sequence".into()),
    }
}

fn yaml_map(value: serde_yaml::Value) -> Result<serde_yaml::Mapping, String> {
    match value {
        serde_yaml::Value::Mapping(map) => Ok(map),
        _ => Err("rule must be a YAML mapping".into()),
    }
}

fn yaml_string(value: serde_yaml::Value) -> Result<String, String> {
    match value {
        serde_yaml::Value::String(value) => Ok(value),
        _ => Err("expected YAML string".into()),
    }
}

fn take_yaml(map: &mut serde_yaml::Mapping, name: &str) -> Result<serde_yaml::Value, String> {
    map.remove(&serde_yaml::Value::String(name.into()))
        .ok_or_else(|| format!("missing ast-rule YAML field {name}"))
}

pub fn query_ast_rule(
    path: &str,
    bytes: &[u8],
    request: &AstRuleRequest,
) -> Result<Vec<AstRuleMatch>, AstRuleError> {
    query_ast_rule_with_content(path, bytes, request, content_id_of(bytes))
}

/// Query bytes supplied by a source host while retaining the source host's
/// content identity in every emitted source/span proposal relation.
pub fn query_ast_rule_with_content(
    path: &str,
    bytes: &[u8],
    request: &AstRuleRequest,
    content: ContentId,
) -> Result<Vec<AstRuleMatch>, AstRuleError> {
    let language =
        RyiLang::from_path(path).ok_or_else(|| AstRuleError::NoGrammar(path.into()))?;
    let source =
        std::str::from_utf8(bytes).map_err(|error| AstRuleError::Utf8(error.to_string()))?;
    unknown_kind(&request.rule, language)
        .into_iter()
        .chain(request.utils.iter().flat_map(|named| unknown_kind(&named.rule, language)))
        .chain(
            request
                .constraints
                .iter()
                .flat_map(|named| unknown_kind(&named.rule, language)),
        )
        .next()
        .map_or(Ok(()), Err)?;
    let config_yaml = serde_yaml::to_string(&ConfigWire::from_request(request, language))
        .map_err(|error| AstRuleError::Yaml(error.to_string()))?;
    let configs: Vec<RuleConfig<RyiLang>> =
        from_yaml_string(&config_yaml, &GlobalRules::default())
            .map_err(|error| AstRuleError::InvalidRule(error.to_string()))?;
    let config = configs
        .into_iter()
        .next()
        .ok_or_else(|| AstRuleError::InvalidRule("empty rule config".into()))?;
    let root = AstGrep::<StrDoc<RyiLang>>::new(source, language);
    let fixer = config
        .get_fixer()
        .map_err(|error| AstRuleError::InvalidRule(error.to_string()))?
        .into_iter()
        .next();
    let mut matches = root
        .root()
        .find_all(&config.matcher)
        .map(|matched| {
            make_match(
                path,
                content.clone(),
                request,
                fixer.as_ref(),
                &config.matcher,
                matched,
            )
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        (&left.query, &left.path, left.span, &left.captures).cmp(&(
            &right.query,
            &right.path,
            right.span,
            &right.captures,
        ))
    });
    matches.dedup();
    Ok(matches)
}

fn make_match(
    path: &str,
    content: ContentId,
    request: &AstRuleRequest,
    fixer: Option<&ast_grep_config::Fixer>,
    matcher: &ast_grep_config::RuleCore,
    matched: NodeMatch<StrDoc<RyiLang>>,
) -> AstRuleMatch {
    let range = matched.range();
    let span = Span {
        start: range.start as u32,
        len: (range.end - range.start) as u32,
    };
    let mut captures = matched
        .get_env()
        .get_matched_variables()
        .filter_map(|variable| {
            let name = match variable {
                MetaVariable::Capture(name, _) | MetaVariable::MultiCapture(name) => name,
                _ => return None,
            };
            matched.get_env().get_match(&name).map(|node| {
                let range = node.range();
                AstRuleCapture {
                    name,
                    text: node.text().into(),
                    span: Span {
                        start: range.start as u32,
                        len: (range.end - range.start) as u32,
                    },
                }
            })
        })
        .collect::<Vec<_>>();
    captures.sort();
    AstRuleMatch {
        record: "ast_rule",
        query: request.id.clone(),
        path: path.into(),
        content: content.clone(),
        span,
        captures,
        proposal: fixer.map(|fixer| {
            let edit = matched.make_edit(matcher, fixer);
            AstRuleMutationProposal {
                query: request.id.clone(),
                content,
                span: Span {
                    start: edit.position as u32,
                    len: edit.deleted_length as u32,
                },
                replacement: String::from_utf8(edit.inserted_text)
                    .expect("ast-grep UTF-8 fixer replacement"),
            }
        }),
    }
}

#[derive(Serialize)]
struct ConfigWire {
    id: String,
    language: RyiLang,
    rule: RuleWire,
    utils: BTreeMap<String, RuleWire>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    constraints: BTreeMap<String, RuleWire>,
    fix: Option<String>,
}
#[derive(Serialize)]
#[serde(untagged)]
enum RuleWire {
    Map(BTreeMap<String, serde_yaml::Value>),
}

impl ConfigWire {
    fn from_request(request: &AstRuleRequest, language: RyiLang) -> Self {
        Self {
            id: request.id.clone(),
            language,
            rule: rule_wire(&request.rule),
            utils: request
                .utils
                .iter()
                .map(|rule| (rule.id.clone(), rule_wire(&rule.rule)))
                .collect(),
            constraints: request
                .constraints
                .iter()
                .map(|rule| (rule.id.clone(), rule_wire(&rule.rule)))
                .collect(),
            fix: request.fix.clone(),
        }
    }
}

fn rule_wire(rule: &AstRule) -> RuleWire {
    use AstRule::*;
    let mut map = BTreeMap::new();
    match rule {
        Pattern(value) => put(&mut map, "pattern", value),
        Kind(value) => put(&mut map, "kind", value),
        Regex(value) => put(&mut map, "regex", value),
        Matches(value) => put(&mut map, "matches", value),
        All(rules) => put(
            &mut map,
            "all",
            &rules.iter().map(rule_wire).collect::<Vec<_>>(),
        ),
        Any(rules) => put(
            &mut map,
            "any",
            &rules.iter().map(rule_wire).collect::<Vec<_>>(),
        ),
        Not(rule) => put(&mut map, "not", &rule_wire(rule)),
        Inside { rule, stop_by } => relation(&mut map, "inside", rule, stop_by),
        Has { rule, stop_by } => relation(&mut map, "has", rule, stop_by),
        Follows { rule, stop_by } => relation(&mut map, "follows", rule, stop_by),
        Precedes { rule, stop_by } => relation(&mut map, "precedes", rule, stop_by),
        NthChild {
            position,
            of_rule,
            reverse,
        } => {
            let mut inner = BTreeMap::new();
            put(&mut inner, "position", position);
            if let Some(of_rule) = of_rule {
                put(&mut inner, "ofRule", &rule_wire(of_rule));
            }
            put(&mut inner, "reverse", reverse);
            put(&mut map, "nthChild", &RuleWire::Map(inner));
        }
        Range { start, end } => {
            let point = |(line, column): &(u32, u32)| {
                let mut point = BTreeMap::new();
                put(&mut point, "line", line);
                put(&mut point, "column", column);
                point
            };
            let mut inner = BTreeMap::new();
            put(&mut inner, "start", &point(start));
            put(&mut inner, "end", &point(end));
            put(&mut map, "range", &RuleWire::Map(inner));
        }
    }
    RuleWire::Map(map)
}

fn put<T: Serialize>(map: &mut BTreeMap<String, serde_yaml::Value>, key: &str, value: &T) {
    map.insert(
        key.into(),
        serde_yaml::to_value(value).expect("typed rule serialization"),
    );
}
/// Every `kind:` in a rule tree that the grammar does not spell. A grammar
/// enumerates its node kinds, so this is a lookup, never a heuristic.
fn unknown_kind(rule: &AstRule, language: RyiLang) -> Option<AstRuleError> {
    use ast_grep_core::tree_sitter::LanguageExt;
    let grammar = language.get_ts_language();
    let spelled = |name: &str| {
        (0..grammar.node_kind_count())
            .filter_map(|id| grammar.node_kind_for_id(id as u16))
            .any(|kind| kind == name)
    };
    let children: Vec<&AstRule> = match rule {
        AstRule::Kind(name) if !spelled(name) => {
            return Some(AstRuleError::UnknownKind {
                kind: name.clone(),
                language: language.name().into_owned(),
            })
        }
        AstRule::All(list) | AstRule::Any(list) => list.iter().collect(),
        AstRule::Not(inner) => vec![inner.as_ref()],
        AstRule::Inside { rule, stop_by }
        | AstRule::Has { rule, stop_by }
        | AstRule::Follows { rule, stop_by }
        | AstRule::Precedes { rule, stop_by } => match stop_by {
            Some(StopBy::Rule(stop)) => vec![rule.as_ref(), stop.as_ref()],
            _ => vec![rule.as_ref()],
        },
        AstRule::NthChild {
            of_rule: Some(of_rule),
            ..
        } => vec![of_rule.as_ref()],
        _ => Vec::new(),
    };
    children
        .into_iter()
        .find_map(|child| unknown_kind(child, language))
}

fn relation(
    map: &mut BTreeMap<String, serde_yaml::Value>,
    key: &str,
    rule: &AstRule,
    stop_by: &Option<StopBy>,
) {
    let mut inner = match rule_wire(rule) {
        RuleWire::Map(map) => map,
    };
    // `StopBy::Rule` carries an `AstRule`, whose own derive spells the variant
    // name; only `rule_wire` spells the key ast-grep reads.
    match stop_by {
        Some(StopBy::End(value)) => put(&mut inner, "stopBy", value),
        Some(StopBy::Rule(stop)) => put(&mut inner, "stopBy", &rule_wire(stop)),
        None => {}
    }
    put(map, key, &RuleWire::Map(inner));
}
