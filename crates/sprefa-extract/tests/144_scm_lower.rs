//! `.scm` surface lowering, proved by one query that uses every operator the
//! surface reaches, against one committed snapshot.
//!
//! The snapshot carries the lowered rule, the matches, and a census over every
//! variant of `AstRule`, `StopBy` and `ScmLowerError`. A variant the surface
//! cannot reach is named as such, so a change that reaches one arrives as
//! snapshot drift instead of as silence.

use sprefa_extract::lang::{
    lower_scm, query_ast_rule, scm_language, AstRule, AstRuleRequest, ScmLowerError, StopBy,
};

const SNAP: &str = "tests/fixtures/scm/lower.snap";

/// A call that satisfies every relation, a generic whose name the regex rejects,
/// and a call inside a closure that containment rejects.
const SRC: &str = r#"fn plain(name: &str) -> bool {
    let a = 1;
    drop(a);
    name.contains("ab")
}

fn generic<T>(i: &[T]) -> bool { i.is_empty() }

fn wrapped(n: &str) -> bool {
    let f = || n.contains("cd");
    f()
}
"#;

/// Seven labelled patterns become `utils`; the unlabelled one becomes the rule.
/// Alternation, field selectors, a negated field, all four relations, both
/// negated forms, the regex both ways, the sibling position, the line window,
/// and a metavariable constraint, in one query on one capture.
const SCM: &str = r#"[(closure_expression)] @closure
[(function_item) (impl_item)] @scope
[(let_declaration)] @decl
[(field_expression)] @receiver
[(let_declaration)] @neighbor
[(call_expression)] @call
[(identifier)] @ident

((call_expression
   function: (field_expression !arguments field: (field_identifier) @name)) @m
 (#inside? @m scope)
 (#not-inside? @m closure)
 (#has? @m receiver)
 (#not-has? @m closure)
 (#follows? @m decl)
 (#not-precedes? @m decl)
 (#match? @m "contains")
 (#not-match? @m "is_empty")
 (#pattern? @m "$R.contains($A)" R ident)
 (#inside? @m scope receiver)
 (#has? @m receiver "neighbor")
 (#follows? @m neighbor "end")
 (#nth-child? @m "1" call "reverse")
 (#not-nth-child? @m "2n")
 (#range? @m "3:4" "3:23"))
"#;

/// One input per `ScmLowerError` variant the surface can produce. A query either
/// lowers or fails, so these cannot ride inside `SCM`.
const REFUSALS: &[&str] = &[
    "(function_item",
    "(_) @m",
    "((identifier) @m (#nope? @m x))",
    "(block) @s\n\n((identifier) @m (#not-nope? @m s))",
    "(block) @s\n\n((identifier) @m (#inside? @m))",
    "((identifier) @m (#inside? @m no_such))",
    "(function_item) @dup\n\n(let_declaration) @dup",
    "(block) @s\n\n((let_declaration (identifier) @a) (identifier) @b (#inside? @a s) (#inside? @b s))",
    "(block) @s\n\n((identifier) @m (#inside? @m s \"bogus\"))",
    "((identifier) @m (#nth-child? @m \"x\"))",
    "((identifier) @m (#range? @m \"1:0\" \"nope\"))",
    "(block) @s\n(identifier) @i\n\n((call_expression) @m (#pattern? @m \"$A\" A s) (#pattern? @m \"$A.b\" A i))",
];

/// Declaration order in `src/lang/1_ast_rule.rs`.
const AST_RULE_VARIANTS: &[&str] = &[
    "Pattern", "Kind", "Regex", "Matches", "All", "Any", "Not", "Inside", "Has", "Follows",
    "Precedes", "NthChild", "Range",
];

const STOP_BY_VARIANTS: &[&str] = &["End", "Rule"];

/// Declaration order in `src/lang/5_scm_lower.rs`.
const ERROR_VARIANTS: &[&str] = &[
    "Syntax", "UnknownPredicate", "PredicateArity", "UnboundReference", "DuplicateLabel",
    "UnknownStopBy", "FocusConflict", "BadPosition", "ConstraintConflict",
];

/// Variant names a lowered rule uses, appended in tree order.
fn walk(rule: &AstRule, rules: &mut Vec<&'static str>, stops: &mut Vec<&'static str>) {
    let (name, children, stop) = match rule {
        AstRule::Pattern(_) => ("Pattern", Vec::new(), None),
        AstRule::Kind(_) => ("Kind", Vec::new(), None),
        AstRule::Regex(_) => ("Regex", Vec::new(), None),
        AstRule::Matches(_) => ("Matches", Vec::new(), None),
        AstRule::All(list) => ("All", list.iter().collect(), None),
        AstRule::Any(list) => ("Any", list.iter().collect(), None),
        AstRule::Not(inner) => ("Not", vec![inner.as_ref()], None),
        AstRule::Inside { rule, stop_by } => ("Inside", vec![rule.as_ref()], stop_by.as_ref()),
        AstRule::Has { rule, stop_by } => ("Has", vec![rule.as_ref()], stop_by.as_ref()),
        AstRule::Follows { rule, stop_by } => ("Follows", vec![rule.as_ref()], stop_by.as_ref()),
        AstRule::Precedes { rule, stop_by } => ("Precedes", vec![rule.as_ref()], stop_by.as_ref()),
        AstRule::NthChild { of_rule, .. } => (
            "NthChild",
            of_rule.iter().map(|rule| rule.as_ref()).collect(),
            None,
        ),
        AstRule::Range { .. } => ("Range", Vec::new(), None),
    };
    rules.push(name);
    match stop {
        Some(StopBy::End(_)) => stops.push("End"),
        Some(StopBy::Rule(inner)) => {
            stops.push("Rule");
            walk(inner, rules, stops);
        }
        None => {}
    }
    for child in children {
        walk(child, rules, stops);
    }
}

fn error_variant(error: &ScmLowerError) -> &'static str {
    match error {
        ScmLowerError::Syntax { .. } => "Syntax",
        ScmLowerError::UnknownPredicate(_) => "UnknownPredicate",
        ScmLowerError::PredicateArity { .. } => "PredicateArity",
        ScmLowerError::UnboundReference(_) => "UnboundReference",
        ScmLowerError::DuplicateLabel(_) => "DuplicateLabel",
        ScmLowerError::UnknownStopBy(_) => "UnknownStopBy",
        ScmLowerError::FocusConflict { .. } => "FocusConflict",
        ScmLowerError::BadPosition(_) => "BadPosition",
        ScmLowerError::ConstraintConflict { .. } => "ConstraintConflict",
    }
}

fn census(all: &[&str], seen: &[&'static str]) -> String {
    all.iter()
        .map(|variant| match seen.contains(variant) {
            true => format!("  {variant}: reached"),
            false => format!("  {variant}: UNREACHABLE from .scm"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Regenerate with `UPDATE_SNAP=1 cargo test`.
#[test]
fn scm_lowering() {
    let abi = scm_language().abi_version();
    assert!(
        (13..=15).contains(&abi),
        "tree-sitter-tsquery ABI {abi} is outside the 13..=15 window tree-sitter 0.25 accepts"
    );

    let program = lower_scm(SCM).expect("the maximal query lowers");
    let mut rules = Vec::new();
    let mut stops = Vec::new();
    walk(&program.rule, &mut rules, &mut stops);
    for util in &program.utils {
        walk(&util.rule, &mut rules, &mut stops);
    }
    for constraint in &program.constraints {
        walk(&constraint.rule, &mut rules, &mut stops);
    }

    let utils: Vec<&String> = program.utils.iter().map(|util| &util.id).collect();
    let request = AstRuleRequest {
        id: "maximal".into(),
        rule: program.rule.clone(),
        utils: program.utils.clone(),
        constraints: program.constraints.clone(),
        fix: None,
    };
    let rows = query_ast_rule("probe.rs", SRC.as_bytes(), &request)
        .expect("the lowered rule compiles for ast-grep");
    let texts: Vec<&str> = rows
        .iter()
        .map(|row| {
            let start = row.span.start as usize;
            &SRC[start..start + row.span.len as usize]
        })
        .collect();

    let refusals: Vec<String> = REFUSALS
        .iter()
        .map(|scm| match lower_scm(scm) {
            Ok(program) => panic!("{scm:?} lowered to {:?} instead of refusing", program.rule),
            Err(error) => {
                let variant = error_variant(&error);
                format!("  {variant}: {error:?}")
            }
        })
        .collect();
    let errors_seen: Vec<&'static str> = REFUSALS
        .iter()
        .map(|scm| error_variant(&lower_scm(scm).expect_err("refuses")))
        .collect();

    // A kind no grammar spells is refused before the run, per language, and
    // ast-grep would otherwise match nothing for it in silence.
    let kinds: Vec<String> = ["probe.rs", "probe.ts", "probe.py", "probe.pl", "probe.gd"]
        .iter()
        .map(|path| {
            let program = lower_scm("(no_such_node_kind) @m").expect("a bogus kind still lowers");
            let request = AstRuleRequest {
                id: "bogus".into(),
                rule: program.rule,
                utils: program.utils,
                constraints: program.constraints,
                fix: None,
            };
            match query_ast_rule(path, b"x", &request) {
                Ok(rows) => panic!("{path} ran a bogus kind and got {} rows", rows.len()),
                Err(error) => format!("  {path}: {error}"),
            }
        })
        .collect();

    let actual = [
        format!("utils: {utils:?}"),
        format!("constraints: {:?}", program.constraints),
        format!("rule: {:?}", program.rule),
        format!("matches: {}", rows.len()),
        format!("texts: {texts:?}"),
        format!("## refusals\n{}", refusals.join("\n")),
        format!("## unknown kind, per language\n{}", kinds.join("\n")),
        format!("## AstRule census\n{}", census(AST_RULE_VARIANTS, &rules)),
        format!("## StopBy census\n{}", census(STOP_BY_VARIANTS, &stops)),
        format!("## ScmLowerError census\n{}", census(ERROR_VARIANTS, &errors_seen)),
    ]
    .join("\n\n");

    if std::env::var("UPDATE_SNAP").is_ok() {
        std::fs::create_dir_all("tests/fixtures/scm").expect("snap dir");
        std::fs::write(SNAP, format!("{actual}\n")).expect("write snap");
        eprintln!("updated {SNAP}");
        return;
    }
    let expected = std::fs::read_to_string(SNAP).expect("snap missing");
    assert_eq!(
        actual,
        expected.trim_end(),
        "scm lowering snapshot drifted. Regenerate with UPDATE_SNAP=1 cargo test, or overwrite \
         {SNAP} with:\n----\n{actual}\n----",
    );
}
