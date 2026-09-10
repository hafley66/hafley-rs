//! S2 crouch-chart qualification: exact traces, rejection, ordered facts and
//! clone/JSON suffix replay. Every dispatch goes through the redux `Slice`.

use game_fighter::chart::{Action, CrouchChart, CrouchSlice, Facts};
use redux::{Never, Slice};

fn facts(animation_finished: bool, crouch_request: bool, crouch_release: bool) -> Facts {
    Facts {
        animation_finished,
        crouch_request,
        crouch_release,
    }
}

fn dispatch(chart: &mut CrouchChart, f: Facts) -> Action {
    CrouchSlice::reduce(chart, f, (), &mut |_: Never| {})
}

#[test]
fn crouch_lifecycle_exact_trace() {
    let mut chart = CrouchChart::default();
    let mut trace = vec![chart.action()];
    for f in [
        facts(false, true, false),
        facts(false, true, false),
        facts(true, false, false),
        facts(false, false, true),
        facts(true, false, false),
    ] {
        trace.push(dispatch(&mut chart, f));
    }
    assert_eq!(
        trace,
        vec![
            Action::Idle,
            Action::CrouchEnter,
            Action::CrouchEnter,
            Action::CrouchHold,
            Action::CrouchExit,
            Action::Idle,
        ]
    );
}

#[test]
fn rejected_events_leave_the_action_unchanged() {
    let mut chart = CrouchChart::default();
    // Idle rejects completion and release.
    assert_eq!(dispatch(&mut chart, facts(true, false, false)), Action::Idle);
    assert_eq!(dispatch(&mut chart, facts(false, false, true)), Action::Idle);
    // Enter Squat.
    assert_eq!(
        dispatch(&mut chart, facts(false, true, false)),
        Action::CrouchEnter
    );
    // Squat has no crouch-release or repeated-request interrupt.
    assert_eq!(
        dispatch(&mut chart, facts(false, false, true)),
        Action::CrouchEnter
    );
    assert_eq!(
        dispatch(&mut chart, facts(false, true, false)),
        Action::CrouchEnter
    );
    // Squat -> SquatWait.
    assert_eq!(
        dispatch(&mut chart, facts(true, false, false)),
        Action::CrouchHold
    );
    // SquatWait has no completion interrupt and does not re-enter on request.
    assert_eq!(
        dispatch(&mut chart, facts(true, false, false)),
        Action::CrouchHold
    );
    assert_eq!(
        dispatch(&mut chart, facts(false, true, false)),
        Action::CrouchHold
    );
    // SquatWait -> SquatRv.
    assert_eq!(
        dispatch(&mut chart, facts(false, false, true)),
        Action::CrouchExit
    );
    // SquatRv only completes; request and release are rejected.
    assert_eq!(
        dispatch(&mut chart, facts(false, true, false)),
        Action::CrouchExit
    );
    assert_eq!(
        dispatch(&mut chart, facts(false, false, true)),
        Action::CrouchExit
    );
    assert_eq!(dispatch(&mut chart, facts(true, false, false)), Action::Idle);
}

#[test]
fn competing_facts_first_match_order_is_exact() {
    let mut chart = CrouchChart::default();
    let all = facts(true, true, true);
    assert_eq!(dispatch(&mut chart, all), Action::CrouchEnter);
    assert_eq!(dispatch(&mut chart, all), Action::CrouchHold);
    assert_eq!(dispatch(&mut chart, all), Action::CrouchExit);
    assert_eq!(dispatch(&mut chart, all), Action::Idle);
}

#[test]
fn clone_and_json_suffix_replay_match() {
    let mut chart = CrouchChart::default();
    for f in [facts(false, true, false), facts(true, false, false)] {
        dispatch(&mut chart, f);
    }
    assert_eq!(chart.action(), Action::CrouchHold);

    let mut cloned = chart.clone();
    let mut decoded: CrouchChart =
        serde_json::from_slice(&serde_json::to_vec(&chart).unwrap()).unwrap();
    // No entry/exit side effects: Serde's reset `initialized` bit is not logical
    // state, so the decoded chart equals the clone.
    assert_eq!(decoded, cloned);
    assert_eq!(decoded, chart);

    let suffix = [
        facts(false, false, true),
        facts(true, false, false),
        facts(false, true, false),
        facts(true, false, false),
    ];
    let mut clone_trace = Vec::new();
    let mut decoded_trace = Vec::new();
    for f in suffix {
        clone_trace.push(dispatch(&mut cloned, f));
        decoded_trace.push(dispatch(&mut decoded, f));
        assert_eq!(cloned, decoded);
    }
    assert_eq!(clone_trace, decoded_trace);
    assert_eq!(
        clone_trace,
        vec![
            Action::CrouchExit,
            Action::Idle,
            Action::CrouchEnter,
            Action::CrouchHold,
        ]
    );
    assert_eq!(cloned.action(), Action::CrouchHold);
}
