use anyhow::{Context, Result};
use boop::{bus, Registry};
use clap::Subcommand;
use std::{fs::OpenOptions, path::PathBuf, time::Duration};

#[derive(Subcommand)]
pub(crate) enum ReminderCmd {
    /// Persist a named schedule. First delivery is one interval from now.
    Add {
        name: String,
        /// Existing explicit route name (no parent/children aliases).
        route: String,
        body: String,
        /// Positive interval: seconds, or suffix s/m/h (for example 30m).
        #[arg(long, value_parser = interval)]
        every: i64,
        /// Required exclusive expiry: Unix seconds or RFC3339 with offset.
        #[arg(long, value_parser = expiry)]
        until: i64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// JSON schedules, outstanding message ids and latest scheduler detail.
    List {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Stop future delivery. Already accepted harness work cannot be retracted.
    Cancel {
        name: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Foreground runner, one per mail dir; restart resumes persisted due times.
    /// At most one outstanding occurrence per route; a turn-end or threaded reply
    /// releases it. Unknown/crashed delivery stays outstanding (inspect with db).
    /// Missed intervals collapse to one. No agents are spawned. Prefer foreground
    /// for native doors; --once needs separately persisted completion receipts.
    Run {
        #[arg(long)]
        once: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

fn interval(s: &str) -> Result<i64, String> {
    let (digits, scale) = match s.as_bytes().last() {
        Some(b's') => (&s[..s.len() - 1], 1000),
        Some(b'm') => (&s[..s.len() - 1], 60_000),
        Some(b'h') => (&s[..s.len() - 1], 3_600_000),
        _ => (s, 1000),
    };
    digits
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(scale))
        .filter(|n| *n > 0)
        .ok_or_else(|| "expected positive seconds or s/m/h interval".into())
}
fn expiry(s: &str) -> Result<i64, String> {
    if let Ok(n) = s.parse::<i64>() {
        return n.checked_mul(1000).ok_or_else(|| "expiry overflow".into());
    }
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
        .map_err(|e| e.to_string())
}

pub(crate) fn run(registry: &Registry, cmd: ReminderCmd) -> Result<()> {
    match cmd {
        ReminderCmd::Add {
            name,
            route,
            body,
            every,
            until,
            mail_dir,
        } => {
            let dir = super::mail_dir(mail_dir.as_deref())?;
            let store = bus::open_store(&dir)?;
            store.reminder_expire(boop::live::now_ms() as i64)?;
            store.reminder_add(
                &name,
                &route,
                &body,
                every,
                until,
                boop::live::now_ms() as i64,
            )?;
            println!(
                "reminder {name}; run: boop beep remind run --mail-dir {}",
                dir.display()
            );
        }
        ReminderCmd::List { mail_dir } => {
            let store = bus::open_store(&super::mail_dir(mail_dir.as_deref())?)?;
            store.reminder_expire(boop::live::now_ms() as i64)?;
            println!("{}", serde_json::to_string_pretty(&store.reminders()?)?);
        }
        ReminderCmd::Cancel { name, mail_dir } => {
            let store = bus::open_store(&super::mail_dir(mail_dir.as_deref())?)?;
            anyhow::ensure!(store.reminder_cancel(&name)?, "no active reminder {name}");
            println!("cancelled {name}");
        }
        ReminderCmd::Run { once, mail_dir } => {
            let dir = super::mail_dir(mail_dir.as_deref())?;
            let store = bus::open_store(&dir)?;
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(dir.join("reminder.lock"))?;
            lock.try_lock()
                .context("another reminder runner owns this mail directory")?;
            let mut observers: std::collections::BTreeMap<String, std::thread::JoinHandle<()>> =
                std::collections::BTreeMap::new();
            loop {
                observers.retain(|_, thread| !thread.is_finished());
                let now = boop::live::now_ms() as i64;
                store.reminder_expire(now)?;
                let schedules = store.reminders()?;
                if !schedules.iter().any(|r| r.state == "active") {
                    break;
                }
                let routes = bus::routes_in(&store)?;
                for reminder in schedules.iter().filter(|r| r.state == "active").take(32) {
                    // A missing/dead lane must not revive or launch anything.
                    let Some(route) = routes.get(&reminder.route) else {
                        store.reminder_detail(&reminder.name, "route unavailable")?;
                        continue;
                    };
                    if route.mode.as_deref() == Some("acpx") {
                        store.reminder_detail(&reminder.name, "acpx unavailable: existing queue enables approve-all; use a native door or supervised lane")?;
                        continue;
                    }
                    if route.kind == "lane"
                        && !route
                            .tmux
                            .as_deref()
                            .is_some_and(|t| boop::tmux::mux().target_alive(None, t))
                    {
                        store.reminder_detail(&reminder.name, "lane unavailable; no revive")?;
                        continue;
                    }
                    if !once && route.kind != "lane" && observers.len() >= 32 {
                        store.reminder_detail(
                            &reminder.name,
                            "completion observer capacity reached",
                        )?;
                        continue;
                    }
                    let now = boop::live::now_ms() as i64;
                    let message = store.reminder_claim(&reminder.name, now)?;
                    let message = if message.is_none() && route.kind != "lane" {
                        store.reminder_retry(&reminder.name, now)?
                    } else {
                        message
                    };
                    if let Some(message) = message {
                        // Persisted before crossing the harness boundary. Restart never
                        // replays an uncertain attempt; existing delivery receipts decide.
                        let result = boop::mail::deliver_hail(registry, &store, &routes, &message);
                        let detail = match result {
                            Ok(landing) => format!(
                                "{}: {} ({})",
                                message.id,
                                landing.outcome(),
                                landing.detail()
                            ),
                            Err(e) => format!("{}: delivery uncertain: {e:#}", message.id),
                        };
                        store.reminder_detail(&reminder.name, &detail)?;
                        println!("{} {detail}", reminder.name);
                    }
                    // Lane supervisors already record turn-ended. Doors expose their
                    // own idle notification. A foreground runner keeps the subscription
                    // open through expiry, avoiding gaps between Codex notifications.
                    let current = store
                        .reminders()?
                        .into_iter()
                        .find(|r| r.name == reminder.name)
                        .unwrap();
                    if let (Some(id), Some(harness)) =
                        (current.last_message.as_deref(), route.harness)
                    {
                        let receipts = store.delivery_rows(id)?;
                        if receipts.iter().any(|r| r.outcome == "accepted-by-harness")
                            && !receipts.iter().any(|r| r.outcome == "turn-ended")
                            && route.kind != "lane"
                            && !observers.contains_key(id)
                        {
                            if let Ok(Some(live)) = boop::mail::live_session(
                                registry.get(harness),
                                &store,
                                route,
                                harness,
                            ) {
                                let accepted_at = receipts
                                    .iter()
                                    .filter(|r| r.outcome == "accepted-by-harness")
                                    .map(|r| r.at_ms)
                                    .max()
                                    .unwrap_or(i64::MAX);
                                let wait_ms = if once {
                                    1000
                                } else {
                                    (current.until_ms - boop::live::now_ms() as i64).max(1) as u64
                                };
                                let dir = dir.clone();
                                let message_id = id.to_owned();
                                let recipient = reminder.route.clone();
                                let observe = move || {
                                    if Registry::discover()
                                        .get(harness)
                                        .door()
                                        .notify_idle(&live, Duration::from_millis(wait_ms))
                                        .is_ok_and(|notice| {
                                            notice.at_ms as i64 > accepted_at
                                                && notice.status_line.as_deref() != Some("gone")
                                        })
                                    {
                                        if let Ok(store) = bus::open_store(&dir) {
                                            let _ = store.append_delivery_transition(
                                                &message_id,
                                                &recipient,
                                                Some(harness),
                                                "turn-ended",
                                                "harness idle notification",
                                                None,
                                                boop::live::now_ms(),
                                            );
                                        }
                                    }
                                };
                                if once {
                                    observe();
                                } else {
                                    observers.insert(id.to_owned(), std::thread::spawn(observe));
                                }
                            }
                        }
                    }
                }
                if once {
                    break;
                }
                std::thread::sleep(Duration::from_millis(700));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_arguments_are_explicit_and_checked() {
        assert_eq!(interval("30m").unwrap(), 1_800_000);
        assert_eq!(
            expiry("1788753600").unwrap(),
            expiry("2026-09-07T00:00:00-04:00").unwrap()
        );
        for bad in ["0", "-1", "forever", "9223372036854775807h"] {
            assert!(interval(bad).is_err());
        }
    }
}
