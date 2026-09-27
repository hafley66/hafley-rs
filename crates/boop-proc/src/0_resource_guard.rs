//! Verified POSIX process-group control for an owned lane agent.

use anyhow::{Context, Result};
use boop_store::proc::ProcReader;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use std::time::{Duration, Instant};

/// One agent process group, anchored to its original leader incarnation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentProcessGroup {
    pgid: i32,
    leader_start_time_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentResourceSample {
    pub rss_bytes: u64,
    pub process_count: usize,
    pub attribution: Duration,
}

impl AgentProcessGroup {
    /// Capture a group whose leader is the child pid started with
    /// `CommandExt::process_group(0)`.
    pub fn capture(leader_pid: u32) -> Result<Self> {
        let snapshot = boop_store::proc::SysinfoSnapshot::capture()?;
        let process = snapshot
            .process(leader_pid)
            .with_context(|| format!("agent process {leader_pid} is not alive"))?;
        let pgid = process_group_id(leader_pid)?;
        anyhow::ensure!(
            pgid == leader_pid as i32,
            "agent process {leader_pid} is not the leader of its own group (pgid {pgid})"
        );
        Ok(Self {
            pgid,
            leader_start_time_secs: process.start_time_secs,
        })
    }

    /// The verified group id, which is also the leader pid.
    pub fn pgid(self) -> i32 {
        self.pgid
    }

    /// Sum RSS for processes currently in the owned group and report sample cost.
    pub fn sample(self) -> Result<AgentResourceSample> {
        let started = Instant::now();
        self.validate_leader()?;
        let members = process_group_members(self.pgid)?;
        let snapshot = boop_store::proc::SysinfoSnapshot::capture()?;
        let rss_bytes = members
            .iter()
            .filter_map(|pid| snapshot.process(*pid))
            .map(|process| process.rss_bytes)
            .sum();
        Ok(AgentResourceSample {
            rss_bytes,
            process_count: members.len(),
            attribution: started.elapsed(),
        })
    }

    /// Stop the agent process group while leaving the supervisor outside it.
    pub fn pause(self) -> Result<()> {
        self.signal(Signal::SIGSTOP)
    }

    /// Continue the previously stopped agent process group.
    pub fn resume(self) -> Result<()> {
        self.signal(Signal::SIGCONT)
    }

    fn signal(self, signal: Signal) -> Result<()> {
        self.validate_leader()?;
        killpg(Pid::from_raw(self.pgid), signal)
            .with_context(|| format!("send {signal:?} to agent process group {}", self.pgid))
    }

    fn validate_leader(self) -> Result<()> {
        let snapshot = boop_store::proc::SysinfoSnapshot::capture()?;
        let process = snapshot
            .process(self.pgid as u32)
            .with_context(|| format!("agent process group leader {} exited", self.pgid))?;
        anyhow::ensure!(
            process.start_time_secs == self.leader_start_time_secs,
            "agent process group leader {} was replaced",
            self.pgid
        );
        anyhow::ensure!(
            process_group_id(self.pgid as u32)? == self.pgid,
            "agent process group {} no longer belongs to its leader",
            self.pgid
        );
        Ok(())
    }
}

fn process_group_members(pgid: i32) -> Result<Vec<u32>> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,pgid="])
        .output()
        .context("list process groups with ps")?;
    anyhow::ensure!(output.status.success(), "ps could not list process groups");
    let mut members = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(process_group)) = (fields.next(), fields.next()) else {
            continue;
        };
        if process_group.parse::<i32>().ok() == Some(pgid) {
            if let Ok(pid) = pid.parse::<u32>() {
                members.push(pid);
            }
        }
    }
    Ok(members)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceGuardConfig {
    pub limit_bytes: Option<u64>,
    pub poll_interval: Duration,
    pub breach_samples: u32,
    pub grace: Duration,
}

impl ResourceGuardConfig {
    pub fn from_env() -> Self {
        let mib = |name: &str| std::env::var(name).ok()?.parse::<u64>().ok();
        let positive = |name: &str, default: u64| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(default)
        };
        let count = |name: &str, default: u32| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(default)
        };
        Self {
            limit_bytes: mib("BOOP_AGENT_RSS_LIMIT_MB")
                .filter(|value| *value > 0)
                .and_then(|value| value.checked_mul(1024 * 1024)),
            poll_interval: Duration::from_millis(positive("BOOP_AGENT_RSS_POLL_MS", 1000)),
            breach_samples: count("BOOP_AGENT_RSS_BREACH_SAMPLES", 3),
            grace: Duration::from_secs(positive("BOOP_AGENT_RSS_GRACE_SECS", 15)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuardPhase {
    Watching { over_limit: u32 },
    Grace { deadline: Instant },
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardAction {
    Interrupt,
    Pause,
}

/// Deterministic state machine for sustained RSS breaches.
pub struct ResourceGuard {
    config: ResourceGuardConfig,
    phase: GuardPhase,
    last_sample: Option<Instant>,
}

impl ResourceGuard {
    pub fn new(config: ResourceGuardConfig) -> Self {
        Self {
            config,
            phase: GuardPhase::Watching { over_limit: 0 },
            last_sample: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.config.limit_bytes.is_some()
    }

    pub fn due(&mut self, now: Instant) -> bool {
        if !self.enabled() || matches!(self.phase, GuardPhase::Paused) {
            return false;
        }
        if self
            .last_sample
            .is_some_and(|last| now.duration_since(last) < self.config.poll_interval)
        {
            return false;
        }
        self.last_sample = Some(now);
        true
    }

    pub fn observe(&mut self, now: Instant, rss_bytes: u64) -> Option<GuardAction> {
        let limit = self.config.limit_bytes?;
        match self.phase {
            GuardPhase::Watching { over_limit } => {
                let over_limit = if rss_bytes > limit { over_limit + 1 } else { 0 };
                if over_limit >= self.config.breach_samples {
                    self.phase = GuardPhase::Grace {
                        deadline: now + self.config.grace,
                    };
                    Some(GuardAction::Interrupt)
                } else {
                    self.phase = GuardPhase::Watching { over_limit };
                    None
                }
            }
            GuardPhase::Grace { deadline } if now >= deadline && rss_bytes > limit => {
                self.phase = GuardPhase::Paused;
                Some(GuardAction::Pause)
            }
            GuardPhase::Grace { deadline } if now >= deadline => {
                self.phase = GuardPhase::Watching { over_limit: 0 };
                None
            }
            GuardPhase::Grace { .. } => None,
            GuardPhase::Paused => None,
        }
    }

    pub fn mark_resumed(&mut self) {
        self.phase = GuardPhase::Watching { over_limit: 0 };
        self.last_sample = None;
    }

    pub fn paused(&self) -> bool {
        matches!(self.phase, GuardPhase::Paused)
    }
}

fn process_group_id(pid: u32) -> Result<i32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .context("read process group id with ps")?;
    anyhow::ensure!(output.status.success(), "ps could not read process {pid}");
    let value = String::from_utf8_lossy(&output.stdout);
    value
        .trim()
        .parse()
        .with_context(|| format!("ps returned an invalid process group id for {pid}: {value:?}"))
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::thread;
    use std::time::Duration;

    use super::AgentProcessGroup;
    use super::{GuardAction, ResourceGuard, ResourceGuardConfig};

    fn state(pid: u32) -> String {
        let output = Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn wait_for_state(pid: u32, stopped: bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let current = state(pid);
            if current.starts_with('T') == stopped {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "state did not settle: {current}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn kill(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    /// RECEIPT. The agent is its own process-group leader; SIGSTOP and SIGCONT
    /// reach it through the same group API the supervisor uses.
    #[test]
    fn pauses_and_resumes_a_child_in_its_own_process_group() {
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        let group = AgentProcessGroup::capture(pid).unwrap();
        assert_eq!(group.pgid(), pid as i32);
        let sample = group.sample().unwrap();
        assert_eq!(sample.process_count, 1);
        assert!(sample.rss_bytes > 0);
        assert!(sample.attribution >= Duration::ZERO);
        let supervisor_pgid = super::process_group_id(std::process::id()).unwrap();
        assert_ne!(group.pgid(), supervisor_pgid);

        group.pause().unwrap();
        wait_for_state(pid, true);
        group.resume().unwrap();
        wait_for_state(pid, false);
        kill(&mut child);
    }

    #[test]
    fn sustained_breach_interrupts_then_pauses_after_grace_and_resets_on_resume() {
        let config = ResourceGuardConfig {
            limit_bytes: Some(100),
            poll_interval: Duration::from_millis(10),
            breach_samples: 2,
            grace: Duration::from_secs(3),
        };
        let mut guard = ResourceGuard::new(config);
        let start = std::time::Instant::now();
        assert!(guard.due(start));
        assert_eq!(guard.observe(start, 101), None);
        assert!(guard.due(start + Duration::from_millis(10)));
        assert_eq!(
            guard.observe(start + Duration::from_millis(10), 101),
            Some(GuardAction::Interrupt)
        );
        assert!(guard.due(start + Duration::from_secs(1)));
        assert_eq!(guard.observe(start + Duration::from_secs(2), 101), None);
        assert_eq!(
            guard.observe(start + Duration::from_secs(4), 101),
            Some(GuardAction::Pause)
        );
        assert!(guard.paused());
        guard.mark_resumed();
        assert!(!guard.paused());
        assert!(guard.due(start + Duration::from_secs(5)));
    }

    #[test]
    fn a_breach_clearing_during_grace_does_not_pause() {
        let config = ResourceGuardConfig {
            limit_bytes: Some(100),
            poll_interval: Duration::from_millis(1),
            breach_samples: 1,
            grace: Duration::from_secs(2),
        };
        let mut guard = ResourceGuard::new(config);
        let start = std::time::Instant::now();
        assert_eq!(guard.observe(start, 101), Some(GuardAction::Interrupt));
        assert_eq!(guard.observe(start + Duration::from_secs(3), 99), None);
        assert!(!guard.paused());
    }
}
