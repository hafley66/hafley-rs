//! The action set the evaluator returns. The evaluator never touches tmux,
//! files, or git; every side effect is owned by the interpreter (section 5 of
//! the plan).

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::fact::Fact;

/// An action the effect interpreter performs. `Send`, `Remind`, and `Skip`
/// route to the resident chat; `Assert`/`Retract` fold into the state file;
/// `Commit` writes that fold to git.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Send a templated message into the resident opencode pane, then Enter.
    Send {
        template: String,
        vars: BTreeMap<String, String>,
    },
    /// Send a reminder text into the same pane.
    Remind { text: String },
    /// Drop the pair and advance the cursor.
    Skip,
    /// Append a fact to the state file.
    Assert(Fact),
    /// Remove a fact from the state file.
    Retract(Fact),
    /// `git add <path> && git commit` after a fold.
    Commit { path: PathBuf, note: String },
}
