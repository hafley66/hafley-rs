//! A route whose session stays dead for `dead_route_attempts()` holds of one
//! row gets `route-dead` on every held row; the drain skips those rows.

use anyhow::Result;

use boop_store::bus::Message;
use boop_store::harness_id::HarnessId;
use boop_store::ident::{DeliveryState, Store};

/// The env knob for how many dead-session holds one row takes before its
/// route is declared dead.
pub const DEAD_ROUTE_ATTEMPTS_ENV: &str = "BOOP_DEAD_ROUTE_ATTEMPTS";

/// The send's own walk plus two drain retries.
const DEAD_ROUTE_ATTEMPTS_DEFAULT: usize = 3;

pub fn dead_route_attempts() -> usize {
    std::env::var(DEAD_ROUTE_ATTEMPTS_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|attempts| *attempts > 0)
        .unwrap_or(DEAD_ROUTE_ATTEMPTS_DEFAULT)
}

/// Whether a mailbox hold's detail names a dead session: the harness reports
/// no live session for the route, or the door transport itself failed.
pub fn names_a_dead_session(detail: &str) -> bool {
    detail.starts_with("no live ") || crate::deliver::door_transport_failure(detail)
}

/// The `held-in-mailbox` transitions of `message_id` on `route` whose detail
/// names a dead session.
pub fn dead_session_holds(store: &Store, message_id: &str, route: &str) -> Result<usize> {
    Ok(store
        .delivery_rows(message_id)?
        .iter()
        .filter(|row| {
            row.route == route
                && row.outcome == DeliveryState::HeldInMailbox.as_str()
                && names_a_dead_session(&row.detail)
        })
        .count())
}

/// A mailbox hold that names a dead session for the bounded count of holds
/// retires every remaining held row of the route.
pub fn retire_if_dead(
    store: &Store,
    route_name: &str,
    harness: Option<HarnessId>,
    rest: &[Message],
    detail: &str,
) {
    let Some(first) = rest.first() else {
        return;
    };
    if !names_a_dead_session(detail) {
        return;
    }
    let attempts = dead_route_attempts();
    match dead_session_holds(store, &first.id, route_name) {
        Ok(holds) if holds >= attempts => {
            if let Err(error) = retire_held(store, route_name, harness, rest, detail, attempts) {
                tracing::warn!(route = route_name, %error, "recording route-dead failed");
            }
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(route = route_name, %error, "reading dead-session holds failed")
        }
    }
}

/// Record `route-dead` for each row in `rows`. Returns the count recorded.
pub fn retire_held(
    store: &Store,
    route: &str,
    harness: Option<HarnessId>,
    rows: &[Message],
    why: &str,
    attempts: usize,
) -> Result<usize> {
    let detail = format!("route dead after {attempts} holds: {why}");
    let now_ms = boop_harness::live::now_ms();
    for message in rows {
        store.append_delivery_transition(
            &message.id,
            route,
            harness,
            DeliveryState::RouteDead.as_str(),
            &detail,
            None,
            now_ms,
        )?;
    }
    if !rows.is_empty() {
        tracing::warn!(
            route,
            rows = rows.len(),
            attempts,
            %why,
            "route dead; held mail stops retrying and stays unread for `boop wait --me`"
        );
    }
    Ok(rows.len())
}
