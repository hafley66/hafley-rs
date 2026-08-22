//! Typed stdin/stdout adapters used by programs that host boop services.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use boop_harness::harness::Harness;
use boop_harness::registry::Registry;
use boop_store::ident::TurnQuery;
use boop_store::rows::TurnRow;

const COMPACT_TOKENS: usize = 100_000;

/// One `boop host chat` stdin row.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ChatRequest {
    pub resident: String,
    pub model: String,
    pub goal: Option<String>,
    pub prompt: String,
}

/// One `boop host chat` stdout row.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ChatResponse {
    Reply {
        reply_turn: i64,
        reply: String,
    },
    Failed {
        outcome: &'static str,
        detail: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SavedChat {
    conversation: Option<String>,
    pending_goal: Option<String>,
}

pub fn run_chat(request: ChatRequest) -> ChatResponse {
    let registry: &'static Registry = Box::leak(Box::new(Registry::discover()));
    match chat_with_registry(request, registry) {
        Ok(response) => response,
        Err(error) => ChatResponse::Failed {
            outcome: "failed",
            detail: format!("{error:#}"),
        },
    }
}

fn chat_with_registry(request: ChatRequest, registry: &'static Registry) -> Result<ChatResponse> {
    let harness = crate::lane::harness_for_model(&request.model)?
        .with_context(|| format!("model `{}` names no harness", request.model))?;
    let adapter = registry.get(harness);
    let root = default_run_dir(&request.resident)?;
    let store_path = boop_store::Store::default_path().context("resolve the default boop store")?;
    run_chat_with_adapter(request, adapter, &root, &store_path)
}

pub fn run_chat_with_adapter(
    request: ChatRequest,
    adapter: &'static dyn Harness,
    root: &Path,
    store_path: &Path,
) -> Result<ChatResponse> {
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let state_path = root.join("chat.json");
    let saved = read_saved(&state_path)?;
    let store = boop_store::Store::open(store_path.to_path_buf()).context("open boop store")?;
    sync_conversation(adapter, &store, saved.conversation.as_deref())?;
    let seen_turn = newest_assistant(&store, saved.conversation.as_deref())?.map(|row| row.turn);
    let goal = saved.pending_goal.or_else(|| {
        saved
            .conversation
            .is_none()
            .then_some(request.goal)
            .flatten()
    });
    // A harness session id can resume a later process, so the channel is reopened per request.
    let mut rewriter = crate::concatmap::Rewriter::open_chat(
        adapter,
        request.model,
        root.to_path_buf(),
        saved.conversation,
        goal,
        COMPACT_TOKENS,
    )?;
    rewriter.rewrite(&store, &request.prompt)?;
    let (conversation, pending_goal) = rewriter
        .chat_state()
        .context("host chat opened a non-chat rewriter")?;
    write_saved(
        &state_path,
        &SavedChat {
            conversation: conversation.clone(),
            pending_goal,
        },
    )?;
    let conversation = conversation.context("resident chat did not name its conversation")?;
    sync_conversation(adapter, &store, Some(&conversation))?;
    let reply = newest_assistant(&store, Some(&conversation))?
        .filter(|row| Some(row.turn) > seen_turn)
        .context("the resident's reply did not reach the boop store")?;
    Ok(ChatResponse::Reply {
        reply_turn: reply.turn,
        reply: crate::concatmap::trim_double_encoded(&reply.said).to_owned(),
    })
}

fn default_run_dir(resident: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !resident.is_empty()
            && resident != "."
            && resident != ".."
            && !resident.contains('/')
            && !resident.contains(std::path::MAIN_SEPARATOR),
        "resident must be one directory name"
    );
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".agent").join("run").join(resident))
}

fn read_saved(path: &Path) -> Result<SavedChat> {
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).with_context(|| format!("read {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SavedChat {
            conversation: None,
            pending_goal: None,
        }),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn write_saved(path: &Path, saved: &SavedChat) -> Result<()> {
    std::fs::write(path, serde_json::to_vec(saved)?)
        .with_context(|| format!("write {}", path.display()))
}

fn sync_conversation(
    adapter: &dyn Harness,
    store: &boop_store::Store,
    conversation: Option<&str>,
) -> Result<()> {
    let Some(conversation) = conversation else {
        return Ok(());
    };
    let Some(session) = adapter
        .sessions()?
        .into_iter()
        .find(|session| session.session_id == conversation)
    else {
        return Ok(());
    };
    boop_harness::sync_session(store, adapter, &session)
        .map(|_| ())
        .context("ingest the resident transcript")
}

fn newest_assistant(
    store: &boop_store::Store,
    conversation: Option<&str>,
) -> Result<Option<TurnRow>> {
    let Some(conversation) = conversation else {
        return Ok(None);
    };
    Ok(store
        .turn_rows(&TurnQuery {
            session: Some(conversation.to_owned()),
            role: Some("assistant".to_owned()),
            ..Default::default()
        })?
        .into_iter()
        .rfind(|row| !row.said.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_rows_have_the_host_contract() {
        assert_eq!(
            serde_json::to_string(&ChatResponse::Reply {
                reply_turn: 7,
                reply: "echo".into(),
            })
            .unwrap(),
            r#"{"reply_turn":7,"reply":"echo"}"#
        );
        assert_eq!(
            serde_json::to_string(&ChatResponse::Failed {
                outcome: "failed",
                detail: "broken".into(),
            })
            .unwrap(),
            r#"{"outcome":"failed","detail":"broken"}"#
        );
    }
}
