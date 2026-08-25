//! What a blocking mail wait watches for, and the next-command line every one
//! of its exits prints. The loop, the clock and the exit code live in the CLI;
//! this module is the selection over bus rows.

use boop_store::bus::{fold, unacked, Message};
use boop_store::DeliveryState;

/// Seconds a wait blocks before exiting 124. Under the 10-minute cap a
/// background shell gives an agent, so the re-run line is always reachable.
pub const DEFAULT_TIMEOUT_SECS: u64 = 540;

/// What one wait is watching for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Watch {
    /// A reply to one sent message.
    Reply { id: String },
    /// The next unread mail addressed to one name.
    Inbox { name: String },
}

impl Watch {
    /// The phrase the timeout line names after "waiting for".
    pub fn what(&self) -> String {
        match self {
            Watch::Reply { id } => format!("reply to {id}"),
            Watch::Inbox { name } => format!("mail for {name}"),
        }
    }

    /// The command that re-arms this exact wait, printed on every exit so the
    /// reader never composes one.
    pub fn command(
        &self,
        timeout_secs: u64,
        as_name: Option<&str>,
        mail_dir: Option<&str>,
    ) -> String {
        let subject = match self {
            Watch::Reply { id } => id.clone(),
            Watch::Inbox { .. } => "--me".to_owned(),
        };
        let mut command = format!("boop wait {subject} --wait-timeout {timeout_secs}");
        if let Some(name) = as_name {
            command.push_str(&format!(" --as {name}"));
        }
        if let Some(dir) = mail_dir {
            command.push_str(&format!(" --mail-dir {dir}"));
        }
        command
    }

    /// The rows this watch is satisfied by, empty while it keeps waiting.
    pub fn arrivals(&self, rows: &[Message]) -> Vec<Message> {
        match self {
            Watch::Reply { id } => reply_to(rows, id).into_iter().collect(),
            Watch::Inbox { name } => unread_for(rows, name),
        }
    }
}

/// The reply to `id`: a row naming it in `reply_to`, or the recipient's next
/// mail back to the sender. An agent that answers without threading the id has
/// still answered. A reply already taken delivery of is history, so a second
/// wait on the same id blocks instead of replaying it.
pub fn reply_to(rows: &[Message], id: &str) -> Option<Message> {
    let folded = fold(rows);
    let sent = folded.iter().find(|row| row.id == id);
    folded
        .iter()
        .filter(|row| row.id != id && row.kind != "ack" && row.to_timestamp.is_none())
        .find(|row| {
            let threaded = row.reply_to.as_deref() == Some(id);
            let answered = sent.is_some_and(|sent| {
                row.from == sent.to
                    && row.to == sent.from
                    && row.from_timestamp > sent.from_timestamp
            });
            threaded || answered
        })
        .cloned()
}

/// Kinds no inbox wait hands back. An `ack` is a fact about the transcript,
/// never mail to read. A `dispatch` is the lane's own spawn brief, already in
/// the first turn, and reading it a second time restarts work already done.
pub const NOT_UNREAD_KINDS: [&str; 2] = ["ack", "dispatch"];

/// Every row addressed to `name` that nothing has taken delivery of.
pub fn unread_for(rows: &[Message], name: &str) -> Vec<Message> {
    unacked(rows)
        .into_iter()
        .filter(|row| row.to == name && !NOT_UNREAD_KINDS.contains(&row.kind.as_str()))
        .collect()
}

/// Whether the ledger says something already put this row in front of its
/// recipient, from the `outcome` words of its delivery transitions.
///
/// `held-in-mailbox` is the one landing that reads as still unread: a row to a
/// route naming no harness stops there, and a polling `wait --me` is the only
/// thing that ever finds it. Every other landing means a harness, a hook, a
/// pane or a parent door took the body.
pub fn already_in_front_of_the_recipient<'a>(outcomes: impl IntoIterator<Item = &'a str>) -> bool {
    outcomes
        .into_iter()
        .any(|word| match DeliveryState::parse(word) {
            Some(DeliveryState::HeldInMailbox) => false,
            Some(state) => state.landed() || state == DeliveryState::TurnStarted,
            None => false,
        })
}

/// The last line a timed-out wait prints, on stdout and on stderr both.
pub fn timeout_line(watch: &Watch, timeout_secs: u64, command: &str) -> String {
    format!(
        "timed out after {timeout_secs}s waiting for {}; re-run: {command}",
        watch.what()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, from: &str, to: &str, timestamp: &str) -> Message {
        Message {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            from_timestamp: timestamp.to_owned(),
            to_timestamp: None,
            kind: "request".to_owned(),
            reply_to: None,
            body: format!("body of {id}"),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    #[test]
    fn a_threaded_reply_answers_the_wait() {
        let sent = row("m-1", "me", "lane", "2026-08-16T00:00:00Z");
        let mut reply = row("m-2", "lane", "me", "2026-08-16T00:00:01Z");
        reply.reply_to = Some("m-1".to_owned());
        let rows = vec![sent, reply.clone()];
        assert_eq!(reply_to(&rows, "m-1"), Some(reply));
    }

    #[test]
    fn an_unthreaded_answer_from_the_recipient_also_answers_it() {
        let sent = row("m-1", "me", "lane", "2026-08-16T00:00:00Z");
        let answer = row("m-2", "lane", "me", "2026-08-16T00:00:01Z");
        let rows = vec![sent, answer.clone()];
        assert_eq!(reply_to(&rows, "m-1"), Some(answer));
    }

    #[test]
    fn mail_that_predates_the_question_is_not_its_answer() {
        let earlier = row("m-0", "lane", "me", "2026-08-15T00:00:00Z");
        let sent = row("m-1", "me", "lane", "2026-08-16T00:00:00Z");
        let rows = vec![earlier, sent];
        assert_eq!(reply_to(&rows, "m-1"), None);
    }

    #[test]
    fn an_ack_of_the_question_is_not_a_reply() {
        let sent = row("m-1", "me", "lane", "2026-08-16T00:00:00Z");
        let mut ack = sent.clone();
        ack.to_timestamp = Some("2026-08-16T00:00:02Z".to_owned());
        let rows = vec![sent, ack];
        assert_eq!(reply_to(&rows, "m-1"), None);
    }

    #[test]
    fn a_reply_already_taken_delivery_of_is_not_replayed() {
        let sent = row("m-1", "me", "lane", "2026-08-16T00:00:00Z");
        let mut reply = row("m-2", "lane", "me", "2026-08-16T00:00:01Z");
        reply.reply_to = Some("m-1".to_owned());
        let mut delivered = reply.clone();
        delivered.to_timestamp = Some("2026-08-16T00:00:02Z".to_owned());
        let rows = vec![sent, reply, delivered];
        assert_eq!(reply_to(&rows, "m-1"), None);
    }

    #[test]
    fn an_inbox_wait_takes_every_unread_row_and_skips_delivered_ones() {
        let unread_one = row("m-1", "coordinator", "me", "2026-08-16T00:00:00Z");
        let unread_two = row("m-2", "other", "me", "2026-08-16T00:00:01Z");
        let mut delivered = row("m-3", "coordinator", "me", "2026-08-16T00:00:02Z");
        delivered.to_timestamp = Some("2026-08-16T00:00:03Z".to_owned());
        let elsewhere = row("m-4", "coordinator", "someone-else", "2026-08-16T00:00:04Z");
        let rows = vec![unread_one.clone(), unread_two.clone(), delivered, elsewhere];
        assert_eq!(unread_for(&rows, "me"), vec![unread_one, unread_two]);
    }

    /// Defect 3 (addendum 2026-08-25): the lane's own dispatch row came back
    /// from `wait --me` as the next unread row, hours after the spawn turn ate
    /// it.
    #[test]
    fn an_inbox_wait_never_hands_back_the_lanes_own_dispatch_row() {
        let mut dispatch = row("m-d2252d54", "coordinator", "me", "2026-08-16T00:00:00Z");
        dispatch.kind = "dispatch".to_owned();
        let mail = row("m-2", "other", "me", "2026-08-16T00:00:01Z");
        let rows = vec![dispatch, mail.clone()];
        assert_eq!(unread_for(&rows, "me"), vec![mail]);
    }

    #[test]
    fn a_row_a_harness_hook_or_pane_already_took_is_not_unread() {
        assert!(already_in_front_of_the_recipient(["accepted-by-harness"]));
        assert!(already_in_front_of_the_recipient([
            "appended",
            "queued-in-hook-inbox"
        ]));
        assert!(already_in_front_of_the_recipient(["pasted-into-pane"]));
        assert!(already_in_front_of_the_recipient(["turn-started"]));
        assert!(already_in_front_of_the_recipient(["parent-door-delivered"]));
    }

    /// A row to a route naming no harness stops at `held-in-mailbox`, and a
    /// polling `wait --me` is the only thing that ever delivers it.
    #[test]
    fn a_row_held_in_the_mailbox_stays_unread() {
        assert!(!already_in_front_of_the_recipient(["held-in-mailbox"]));
        assert!(!already_in_front_of_the_recipient(["appended"]));
        assert!(!already_in_front_of_the_recipient([]));
        assert!(!already_in_front_of_the_recipient([
            "what-boop-writes-next"
        ]));
    }

    #[test]
    fn the_timeout_line_carries_the_command_that_resumes_the_wait() {
        let watch = Watch::Reply {
            id: "m-691bc40e".to_owned(),
        };
        let command = watch.command(540, None, None);
        assert_eq!(command, "boop wait m-691bc40e --wait-timeout 540");
        assert_eq!(
            timeout_line(&watch, 540, &command),
            "timed out after 540s waiting for reply to m-691bc40e; re-run: boop wait m-691bc40e --wait-timeout 540"
        );
    }

    #[test]
    fn an_inbox_wait_resumes_as_me_with_the_name_it_waited_as() {
        let watch = Watch::Inbox {
            name: "soopy-driver".to_owned(),
        };
        assert_eq!(
            watch.command(540, Some("soopy-driver"), None),
            "boop wait --me --wait-timeout 540 --as soopy-driver"
        );
        assert_eq!(watch.what(), "mail for soopy-driver");
    }
}
