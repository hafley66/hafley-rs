const DOC: &str = include_str!("../6_remaining.md");

fn field(name: &str) -> u32 {
    let prefix = format!("{name}=");
    DOC.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("6_remaining.md is missing `{name}=`"))
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("6_remaining.md `{name}` is not an integer"))
}

#[test]
fn audit_counts_reconcile_and_match_the_live_chart() {
    let total = field("source_edges_total");
    let represented = field("represented_source_edges");
    let remaining = field("remaining_source_edges");
    let rules = field("live_rules_total");
    let matching = field("source_matching_rules");
    let local = field("local_policy_rules");

    assert_eq!(
        represented + remaining,
        total,
        "represented + remaining source edges must equal the in-scope total"
    );
    assert_eq!(
        matching + local,
        rules,
        "source-matching + local-policy rules must equal the live rule total"
    );

    let rendered = game_fighter::ground_chart::render();
    assert_eq!(
        rendered.matches(" --> ").count() as u32,
        rules,
        "generated chart arrows must equal live_rules_total"
    );

    let remaining_rows = DOC
        .lines()
        .filter(|line| {
            line.starts_with("| ")
                && line[2..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
        })
        .count() as u32;
    assert_eq!(
        remaining_rows, remaining,
        "remaining transition table rows must equal remaining_source_edges"
    );
}
