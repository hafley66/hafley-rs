use std::collections::{BTreeSet, HashMap};

use sprefa_extract::{FamilyTag, FlatFact};

// v5's captured oracle covers flow within a function. Closure capture flow is
// asserted separately so its new edges do not change the captured v5 facts.
pub(crate) fn capture_edges(facts: &[FlatFact]) -> BTreeSet<((u32, u32), (u32, u32))> {
    let owners: HashMap<_, _> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Df,
                span,
                kind,
                function: Some(owner),
                ..
            } => Some(((span.start, span.end, kind.as_str()), owner.as_str())),
            _ => None,
        })
        .collect();
    facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Edge {
                family: FamilyTag::Df,
                from,
                to,
                from_kind: Some(from_kind),
                to_kind: Some(to_kind),
                ..
            } => {
                let source = owners.get(&(from.start, from.end, from_kind.as_str()))?;
                let target = owners.get(&(to.start, to.end, to_kind.as_str()))?;
                target
                    .strip_prefix(*source)
                    .is_some_and(|suffix| suffix.starts_with("::closure::"))
                    .then_some(((from.start, from.end), (to.start, to.end)))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn captures_extend_the_preserved_v5_oracle() {
    let facts = super::v5_support::facts(
        "crates/sprefa-extract/tests/fixtures/ts/lambdas.ts",
        include_bytes!("fixtures/ts/lambdas.ts"),
        false,
    );
    let captures: Vec<_> = capture_edges(&sprefa_extract::flatten(&facts))
        .into_iter()
        .map(|(from, to)| format!("{}:{} -> {}:{}", from.0, from.1, to.0, to.1))
        .collect();
    insta::assert_debug_snapshot!(captures, @r###"
    [
        "679:689 -> 817:823",
    ]
    "###);
}
