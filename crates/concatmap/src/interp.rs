//! The effect interpreter: the single owner of every side effect. The
//! evaluator never touches tmux, files, or git; this module routes `Send` and
//! `Remind` into the resident chat pane and `Commit` into git. `Assert` and
//! `Retract` are applied to state by the `Host` before they reach here, so the
//! interpreter treats them as a no-op record of the fold.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::action::Action;

/// The chat send surface the interpreter drives. `MuxSink` adapts the
/// `boop_mux::Multiplexer`; tests use a recording fake that needs only these
/// two operations.
pub trait ChatSink {
    fn send_text(&self, body: &str) -> Result<()>;
    fn send_key(&self, key: &str) -> Result<()>;
}

/// A `ChatSink` over the shared tmux `Multiplexer`.
pub struct MuxSink<'a> {
    mux: &'a dyn boop_mux::Multiplexer,
    socket: Option<&'a str>,
    pane: String,
}

impl<'a> MuxSink<'a> {
    pub fn new(
        mux: &'a dyn boop_mux::Multiplexer,
        socket: Option<&'a str>,
        pane: String,
    ) -> MuxSink<'a> {
        MuxSink { mux, socket, pane }
    }
}

impl ChatSink for MuxSink<'_> {
    fn send_text(&self, body: &str) -> Result<()> {
        self.mux.send_text(self.socket, &self.pane, body)
    }
    fn send_key(&self, key: &str) -> Result<()> {
        self.mux.send_key_named(self.socket, &self.pane, key)
    }
}

/// The interpreter. Owns the chat sink and the git worktree root.
pub struct Effects<S> {
    pub sink: S,
    pub worktree: std::path::PathBuf,
}

impl<S: ChatSink> Effects<S> {
    /// Apply one action. `Send`/`Remind` go to the pane; `Commit` runs git;
    /// `Skip`/`Assert`/`Retract` are no-ops here.
    pub fn apply(&self, action: &Action) -> Result<()> {
        match action {
            Action::Send { template, vars } => {
                let text = render_template(template, vars);
                self.sink.send_text(&text)?;
                self.sink.send_key("Enter")?;
                Ok(())
            }
            Action::Remind { text } => {
                self.sink.send_text(text)?;
                self.sink.send_key("Enter")?;
                Ok(())
            }
            Action::Skip => Ok(()),
            Action::Assert(_) | Action::Retract(_) => Ok(()),
            Action::Commit { path, note } => git_commit(&self.worktree, path, note),
        }
    }
}

/// Substitute `{{key}}` placeholders in `template` from `vars`. Unknown keys
/// are left verbatim so a typo is visible in the pane, never silently empty.
pub fn render_template(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        match after.find("}}") {
            Some(close) => {
                let key = &after[..close];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&format!("{{{{{key}}}}}")),
                }
                rest = &after[close + 2..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `git add <path> && git commit` in `worktree`. The commit history of the
/// state file is the analysis corpus.
pub fn git_commit(worktree: &Path, path: &Path, note: &str) -> Result<()> {
    let rel = path
        .strip_prefix(worktree)
        .unwrap_or(path)
        .display()
        .to_string();
    let add = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["add", "--", &rel])
        .output()
        .context("git add state file")?;
    if !add.status.success() {
        anyhow::bail!(
            "git add {rel}: {}",
            String::from_utf8_lossy(&add.stderr).trim()
        );
    }
    let commit = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["commit", "-m", note])
        .output()
        .context("git commit state file")?;
    if !commit.status.success() {
        let stdout = String::from_utf8_lossy(&commit.stdout);
        let stderr = String::from_utf8_lossy(&commit.stderr);
        tracing::debug!(stdout = %stdout, stderr = %stderr, "git commit non-zero");
        if !stderr.contains("nothing to commit") && !stdout.contains("nothing to commit") {
            anyhow::bail!("git commit {rel}: {stderr}");
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::process::Command;
    use std::rc::Rc;

    use crate::action::Action;
    use crate::interp::{render_template, ChatSink, Effects};

    /// A recording sink capturing byte-exact text and key order. `Clone` shares
    /// the recorded calls so a caller can inspect after an owned move into
    /// `Effects`.
    #[derive(Default, Clone)]
    pub(crate) struct RecordingSink {
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl RecordingSink {
        pub(crate) fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl ChatSink for RecordingSink {
        fn send_text(&self, body: &str) -> anyhow::Result<()> {
            self.calls.borrow_mut().push(format!("TEXT:{body}"));
            Ok(())
        }
        fn send_key(&self, key: &str) -> anyhow::Result<()> {
            self.calls.borrow_mut().push(format!("KEY:{key}"));
            Ok(())
        }
    }

    fn temp_repo(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "concatmap_git_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let cmds: &[&[&str]] = &[
            &["init", "-q"],
            &["config", "user.email", "pipe@local"],
            &["config", "user.name", "pipe"],
        ];
        for cmd in cmds {
            let status = Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(*cmd)
                .status()
                .unwrap();
            assert!(status.success());
        }
        path
    }

    #[test]
    fn render_replaces_known_keys_and_leaves_unknown() {
        let mut vars = BTreeMap::new();
        vars.insert("user_text".to_owned(), "rewrite it".to_owned());
        let out = render_template("You said: {{user_text}} {{missing}}", &vars);
        assert_eq!(out, "You said: rewrite it {{missing}}");
    }

    #[test]
    fn send_fidelity_is_byte_exact() {
        let sink = RecordingSink::default();
        let effects = Effects {
            sink: sink.clone(),
            worktree: PathBuf::from("/tmp"),
        };
        let text = "a \"quote\"\nline `tick`";
        let mut vars = BTreeMap::new();
        vars.insert("body".to_owned(), text.to_owned());
        effects
            .apply(&Action::Send {
                template: "{{body}}".to_owned(),
                vars,
            })
            .unwrap();
        let calls = sink.calls();
        assert_eq!(calls[0], format!("TEXT:{text}"));
        assert_eq!(calls[1], "KEY:Enter");
    }

    #[test]
    fn commit_writes_one_commit_and_noop_commit_is_not_an_error() {
        let repo = temp_repo("commit");
        let state = repo.join("state/agent.dl6");
        std::fs::create_dir_all(state.parent().unwrap()).unwrap();
        std::fs::write(&state, "fact agent(id=\"x\").\n").unwrap();
        // First commit writes.
        let effects = Effects {
            sink: RecordingSink::default(),
            worktree: repo.clone(),
        };
        effects
            .apply(&Action::Commit {
                path: state.clone(),
                note: "fold".to_owned(),
            })
            .unwrap();
        // Re-committing with no change is not an error.
        effects
            .apply(&Action::Commit {
                path: state.clone(),
                note: "noop".to_owned(),
            })
            .unwrap();
        let log = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        let count = String::from_utf8_lossy(&log.stdout)
            .lines()
            .count();
        assert_eq!(count, 1, "only one real commit");
        let _ = std::fs::remove_dir_all(&repo);
    }
}
