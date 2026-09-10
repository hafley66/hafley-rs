use game_fighter::ground_chart::render;

const GENERATED: &str = include_str!("../5_ground_chart.md");

#[test]
fn generated_ground_chart_is_current() {
    assert_eq!(render(), GENERATED);
}

#[test]
fn generated_ground_chart_exposes_ground_policy_and_none_semantics() {
    let chart = render();
    assert!(chart.contains("Idle --> Crouch: GroundIntent [down] (64/128 facts)"));
    assert!(chart.contains("Dash --> Dash: Motion [reverse] (64/128 facts)"));
    assert!(chart.contains("Crouch -.\"Motion [down] (64/128 facts)\".-> Handled"));
    assert!(chart.contains("`Handled` means `decide` returned `None`"));
}
