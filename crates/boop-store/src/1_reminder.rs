//! Durable schedules over the existing mailbox. A claim and its envelope commit
//! together. Uncertain delivery after a crash stays outstanding until a receipt.
use crate::{bus, Store};
use anyhow::{ensure, Result};
use rusqlite::params;
use serde::Serialize;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS agent_reminder (
 name TEXT PRIMARY KEY, route TEXT NOT NULL, body TEXT NOT NULL,
 every_ms INTEGER NOT NULL CHECK(every_ms > 0), until_ms INTEGER NOT NULL,
 next_ms INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'active',
 last_message TEXT, detail TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_reminder_active_route
 ON agent_reminder(route) WHERE state = 'active';
";

#[derive(Debug, Serialize)]
pub struct Reminder {
    pub name: String,
    pub route: String,
    pub body: String,
    pub every_ms: i64,
    pub until_ms: i64,
    pub next_ms: i64,
    pub state: String,
    pub last_message: Option<String>,
    pub detail: String,
}

impl Store {
    pub fn reminder_add(
        &self,
        name: &str,
        route: &str,
        body: &str,
        every_ms: i64,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<()> {
        let tx = self.connection().unchecked_transaction()?;
        ensure!(
            !name.trim().is_empty() && !body.trim().is_empty(),
            "name and body must be nonempty"
        );
        ensure!(
            every_ms > 0 && until_ms > now_ms,
            "positive interval and future expiry required"
        );
        ensure!(
            bus::routes_in(self)?.contains_key(route),
            "no registered route {route}"
        );
        ensure!(
            self.reminders()?
                .iter()
                .filter(|r| r.state == "active")
                .count()
                < 32,
            "at most 32 active reminders per mail directory"
        );
        self.connection().execute(
            "INSERT INTO agent_reminder(name,route,body,every_ms,until_ms,next_ms) VALUES (?1,?2,?3,?4,?5,?6)",
            params![name, route, body, every_ms, until_ms, now_ms.checked_add(every_ms).ok_or_else(|| anyhow::anyhow!("interval overflow"))?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn reminders(&self) -> Result<Vec<Reminder>> {
        let mut stmt = self.connection().prepare("SELECT name,route,body,every_ms,until_ms,next_ms,state,last_message,detail FROM agent_reminder ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Reminder {
                    name: r.get(0)?,
                    route: r.get(1)?,
                    body: r.get(2)?,
                    every_ms: r.get(3)?,
                    until_ms: r.get(4)?,
                    next_ms: r.get(5)?,
                    state: r.get(6)?,
                    last_message: r.get(7)?,
                    detail: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn reminder_cancel(&self, name: &str) -> Result<bool> {
        Ok(self.connection().execute("UPDATE agent_reminder SET state='cancelled', detail='cancelled by caller' WHERE name=?1 AND state='active'", [name])? > 0)
    }

    pub fn reminder_expire(&self, now_ms: i64) -> Result<()> {
        self.connection().execute("UPDATE agent_reminder SET state='expired', detail='expiry reached' WHERE state='active' AND until_ms<=?1", [now_ms])?;
        Ok(())
    }

    pub fn reminder_detail(&self, name: &str, detail: &str) -> Result<()> {
        self.connection().execute(
            "UPDATE agent_reminder SET detail=?2 WHERE name=?1 AND detail<>?2",
            params![name, detail],
        )?;
        Ok(())
    }

    /// Called under the runner's OS lock. The transaction commits the due time
    /// and envelope together. Only a completed occurrence advances.
    pub fn reminder_claim(&self, name: &str, now_ms: i64) -> Result<Option<bus::Message>> {
        let tx = self.connection().unchecked_transaction()?;
        let claimed = tx.execute(
            "UPDATE agent_reminder SET next_ms=?2 + every_ms, last_message=?3, detail='claimed; awaiting delivery receipt'
             WHERE name=?1 AND state='active' AND next_ms<=?2 AND until_ms>?2
             AND (last_message IS NULL OR EXISTS (
                 SELECT 1 FROM agent_delivery_transition t WHERE t.message_id=last_message AND t.outcome IN ('turn-ended','reply-appended')
             ) OR EXISTS (SELECT 1 FROM agent_mail m WHERE m.reply_to=last_message AND m.kind='reply' AND m.from_route=route))
             AND NOT EXISTS (
               SELECT 1 FROM agent_reminder other WHERE other.route=agent_reminder.route AND other.name<>agent_reminder.name
               AND other.last_message IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM agent_delivery_transition t WHERE t.message_id=other.last_message AND t.outcome IN ('turn-ended','reply-appended'))
               AND NOT EXISTS (SELECT 1 FROM agent_mail m WHERE m.reply_to=other.last_message AND m.kind='reply' AND m.from_route=other.route)
               AND (SELECT outcome FROM agent_delivery_transition t WHERE t.message_id=other.last_message ORDER BY sequence DESC LIMIT 1)
                 IN ('appended','submitted-to-harness','accepted-by-harness')
             )",
            params![name, now_ms, bus::mint_id()],
        )?;
        if claimed == 0 {
            return Ok(None);
        }
        let reminder = self
            .reminders()?
            .into_iter()
            .find(|r| r.name == name)
            .unwrap();
        let message = bus::Message {
            id: reminder.last_message.unwrap(),
            from: format!("reminder:{name}"),
            to: reminder.route,
            from_timestamp: bus::now_iso(),
            to_timestamp: None,
            kind: "request".into(),
            reply_to: None,
            body: reminder.body,
            r#ref: Some(format!("reminder:{name}")),
            rc: None,
            detail: None,
        };
        bus::write_message(self, "bus", &message, "reminder")?;
        tx.commit()?;
        Ok(Some(message))
    }

    /// Retry only a recorded refusal, using the same envelope. Persist the
    /// attempt before crossing the door, so a crash cannot replay it blindly.
    pub fn reminder_retry(&self, name: &str, now_ms: i64) -> Result<Option<bus::Message>> {
        let tx = self.connection().unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE agent_reminder SET next_ms=?2+every_ms
             WHERE name=?1 AND state='active' AND until_ms>?2 AND next_ms<=?2
               AND (SELECT outcome FROM agent_delivery_transition WHERE message_id=last_message ORDER BY sequence DESC LIMIT 1)
                 IN ('held-in-mailbox','held-for-turn-boundary','cooled-off','rejected-by-harness')
               AND EXISTS(SELECT 1 FROM agent_mail WHERE message_id=last_message AND to_timestamp IS NULL)
               AND NOT EXISTS(SELECT 1 FROM agent_delivery_transition WHERE message_id=last_message AND outcome IN ('accepted-by-harness','turn-ended'))",
            params![name, now_ms],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let id: String = tx.query_row(
            "SELECT last_message FROM agent_reminder WHERE name=?1",
            [name],
            |r| r.get(0),
        )?;
        let message = bus::messages_in(self)?.into_iter().find(|m| m.id == id);
        if let Some(message) = &message {
            self.append_delivery_transition(
                &id,
                &message.to,
                None,
                "submitted-to-harness",
                "reminder retry claimed; outcome uncertain until receipt",
                None,
                now_ms as u64,
            )?;
        }
        tx.commit()?;
        Ok(message)
    }

    /// Mail readers check the schedule at read time, even with no runner alive.
    /// Cancellation/expiry cannot retract a body already accepted by a harness.
    pub fn reminder_message_active(&self, message: &bus::Message, now_ms: i64) -> Result<bool> {
        let Some(name) = message
            .r#ref
            .as_deref()
            .and_then(|r| r.strip_prefix("reminder:"))
        else {
            return Ok(true);
        };
        Ok(self.connection().query_row("SELECT EXISTS(SELECT 1 FROM agent_reminder WHERE name=?1 AND state='active' AND until_ms>?2 AND last_message=?3)", params![name,now_ms,message.id], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    const NOW: i64 = 4_000_000_000_000;
    struct Fixture {
        dir: std::path::PathBuf,
        store: Store,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "boop-reminder-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            bus::write_route(
                &dir,
                "recipient",
                &bus::route_from_value(&serde_json::json!({"kind":"native"})),
            )
            .unwrap();
            let store = bus::open_store(&dir).unwrap();
            store
                .reminder_add("tick", "recipient", "bounded work", 1000, NOW + 10_000, NOW)
                .unwrap();
            Self { dir, store }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    #[test]
    fn due_expiry_and_missed_intervals() {
        let f = Fixture::new();
        assert!(f.store.reminder_claim("tick", NOW + 999).unwrap().is_none());
        let m = f.store.reminder_claim("tick", NOW + 5000).unwrap().unwrap();
        assert_eq!(f.store.reminders().unwrap()[0].next_ms, NOW + 6000);
        assert!(f
            .store
            .reminder_claim("tick", NOW + 5000)
            .unwrap()
            .is_none());
        assert!(!f.store.reminder_message_active(&m, NOW + 10_000).unwrap());
        f.store.reminder_expire(NOW + 10_000).unwrap();
        assert_eq!(f.store.reminders().unwrap()[0].state, "expired");
        assert!(f
            .store
            .reminder_claim("tick", NOW + 10_000)
            .unwrap()
            .is_none());
    }
    #[test]
    fn cancellation_hides_held_mail_without_deleting_history() {
        let f = Fixture::new();
        let m = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        assert_eq!(bus::held_messages(&f.store, "recipient").unwrap().len(), 1);
        assert!(f.store.reminder_cancel("tick").unwrap());
        assert!(!f.store.reminder_cancel("tick").unwrap());
        assert!(bus::held_messages(&f.store, "recipient")
            .unwrap()
            .is_empty());
        assert_eq!(bus::messages_in(&f.store).unwrap()[0].id, m.id);
        assert!(bus::unacked(&bus::messages_in(&f.store).unwrap()).is_empty());
        assert!(f
            .store
            .reminder_claim("tick", NOW + 2000)
            .unwrap()
            .is_none());
    }
    #[test]
    fn restart_and_other_connections_do_not_repeat_uncertain_delivery() {
        let f = Fixture::new();
        let m = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        let reopened = bus::open_store(&f.dir).unwrap();
        assert!(reopened
            .reminder_claim("tick", NOW + 3000)
            .unwrap()
            .is_none());
        assert_eq!(
            reopened.reminders().unwrap()[0].last_message.as_deref(),
            Some(m.id.as_str())
        );
        assert_eq!(bus::messages_in(&reopened).unwrap().len(), 1);
    }
    #[test]
    fn admission_does_not_release_overlap_but_turn_end_does() {
        let f = Fixture::new();
        let m = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        f.store
            .append_delivery_transition(
                &m.id,
                "recipient",
                None,
                "accepted-by-harness",
                "fixture",
                None,
                NOW as u64,
            )
            .unwrap();
        bus::ack_messages(&f.store, std::slice::from_ref(&m.id), "taken").unwrap();
        assert!(f
            .store
            .reminder_claim("tick", NOW + 2000)
            .unwrap()
            .is_none());
        f.store
            .append_delivery_transition(
                &m.id,
                "recipient",
                None,
                "turn-ended",
                "fixture",
                None,
                NOW as u64,
            )
            .unwrap();
        let next = f.store.reminder_claim("tick", NOW + 3000).unwrap().unwrap();
        assert_ne!(next.id, m.id);
        assert_eq!(bus::messages_in(&f.store).unwrap().len(), 2);
    }
    #[test]
    fn explicit_threaded_reply_releases_occurrence() {
        let f = Fixture::new();
        let m = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        let reply = bus::Message {
            id: bus::mint_id(),
            from: "recipient".into(),
            to: m.from.clone(),
            reply_to: Some(m.id.clone()),
            kind: "reply".into(),
            ..m
        };
        bus::insert_message(&f.store, "bus", &reply, "fixture").unwrap();
        assert!(f
            .store
            .reminder_claim("tick", NOW + 2000)
            .unwrap()
            .is_some());
    }
    #[test]
    fn known_refusal_retries_same_envelope_but_crashed_retry_does_not() {
        let f = Fixture::new();
        let message = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        assert!(f
            .store
            .reminder_retry("tick", NOW + 2000)
            .unwrap()
            .is_none());
        f.store
            .append_delivery_transition(
                &message.id,
                "recipient",
                None,
                "cooled-off",
                "fixture cooldown",
                None,
                NOW as u64,
            )
            .unwrap();
        assert!(f
            .store
            .reminder_retry("tick", NOW + 1999)
            .unwrap()
            .is_none());
        let retry = f.store.reminder_retry("tick", NOW + 2000).unwrap().unwrap();
        assert_eq!(retry.id, message.id);
        assert!(f
            .store
            .reminder_retry("tick", NOW + 3000)
            .unwrap()
            .is_none());
        assert_eq!(bus::messages_in(&f.store).unwrap().len(), 1);
    }

    #[test]
    fn cancellation_and_replacement_do_not_overlap_accepted_work() {
        let f = Fixture::new();
        let message = f.store.reminder_claim("tick", NOW + 1000).unwrap().unwrap();
        f.store
            .append_delivery_transition(
                &message.id,
                "recipient",
                None,
                "accepted-by-harness",
                "fixture",
                None,
                NOW as u64,
            )
            .unwrap();
        f.store.reminder_cancel("tick").unwrap();
        f.store
            .reminder_add(
                "replacement",
                "recipient",
                "body",
                1000,
                NOW + 9000,
                NOW + 1000,
            )
            .unwrap();
        assert!(f
            .store
            .reminder_claim("replacement", NOW + 2000)
            .unwrap()
            .is_none());
        f.store
            .append_delivery_transition(
                &message.id,
                "recipient",
                None,
                "turn-ended",
                "fixture",
                None,
                NOW as u64,
            )
            .unwrap();
        assert!(f
            .store
            .reminder_claim("replacement", NOW + 3000)
            .unwrap()
            .is_some());
    }

    #[test]
    fn one_active_schedule_per_route_and_missing_route_validation() {
        let f = Fixture::new();
        assert!(f
            .store
            .reminder_add("second", "recipient", "body", 1000, NOW + 2000, NOW)
            .is_err());
        assert!(f
            .store
            .reminder_add("missing", "absent", "body", 1000, NOW + 2000, NOW)
            .is_err());
        assert!(f
            .store
            .reminder_add("bad", "recipient", "body", 0, NOW + 2000, NOW)
            .is_err());
        f.store.reminder_cancel("tick").unwrap();
        f.store
            .reminder_add("second", "recipient", "body", 1000, NOW + 2000, NOW)
            .unwrap();
        assert_eq!(f.store.reminders().unwrap().len(), 2);
    }
}
