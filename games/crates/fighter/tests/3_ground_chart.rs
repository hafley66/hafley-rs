use game_fighter::ground_chart::render;

const GENERATED: &str = include_str!("../5_ground_chart.md");

#[test]
fn generated_ground_chart_is_current() {
    assert_eq!(render(), GENERATED);
    assert_eq!(
        game_fighter::ground_chart::render_d2(),
        include_str!("../5_ground_chart.d2")
    );
}

#[test]
fn generated_ground_chart_exposes_ground_policy_and_none_semantics() {
    let chart = render();
    assert!(chart.contains("Idle --> Crouch: GroundIntent [down] (64/128 facts)"));
    assert!(chart.contains("Dash --> Dash: Motion [reverse] (64/128 facts)"));
    assert!(chart.contains("| Crouch | `down` | 64/128 |"));
    assert!(!chart.contains("Handled"));
    assert_eq!(chart.matches("```mermaid").count(), 3);
}

#[test]
fn generated_ground_chart_covers_the_shared_phase_inventory() {
    let chart = render();
    for phase in game_fighter::Phase::ALL {
        assert!(chart.contains(phase.name()), "chart omits {}", phase.name());
    }
}
