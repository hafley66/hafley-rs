//! The registry is the ONE place a harness is named. Adding a harness means
//! one `HarnessId` variant, one `impl Harness`, and one line here.

use crate::harness::claude::Claude;
use crate::harness::codex::Codex;
use crate::harness::kimi::Kimi;
use crate::harness::opencode::Opencode;
use crate::harness::{Harness, HarnessId, SessionRef};
use crate::transcript::{Message, SessionMeta};

pub struct Registry {
    harnesses: Vec<Box<dyn Harness>>,
}

impl Registry {
    /// Every built-in harness, in id order.
    pub fn discover() -> Self {
        Registry::with(vec![
            Box::new(Claude),
            Box::new(Codex),
            Box::new(Kimi),
            Box::new(Opencode),
        ])
    }

    /// A registry over exactly these adapters. A later entry shadows an
    /// earlier one carrying the same id, which is how a test substitutes one.
    pub fn with(harnesses: Vec<Box<dyn Harness>>) -> Self {
        let mut harnesses = harnesses;
        harnesses.sort_by_key(|harness| harness.id());
        Registry { harnesses }
    }

    pub fn all(&self) -> &[Box<dyn Harness>] {
        &self.harnesses
    }

    /// The adapter for an id. Total: every variant is registered, so a caller
    /// that already holds a `HarnessId` never handles a `None`.
    pub fn get(&self, id: HarnessId) -> &dyn Harness {
        self.harnesses
            .iter()
            .rev()
            .find(|harness| harness.id() == id)
            .map(|boxed| boxed.as_ref())
            .unwrap_or_else(|| panic!("harness `{id}` is not registered"))
    }

    /// The adapter a CLI argument names. `None` when the text names no harness.
    pub fn by_name(&self, name: &str) -> Option<&dyn Harness> {
        let id = HarnessId::parse(name)?;
        self.harnesses
            .iter()
            .rev()
            .find(|harness| harness.id() == id)
            .map(|boxed| boxed.as_ref())
    }

    /// A named adapter resolves exactly. Omission selects the first registered
    /// adapter, matching the existing dispatch default.
    pub fn resolve(&self, name: Option<&str>) -> anyhow::Result<&dyn Harness> {
        let Some(name) = name else {
            return self
                .harnesses
                .first()
                .map(|boxed| boxed.as_ref())
                .ok_or_else(|| anyhow::anyhow!("no harness registered"));
        };
        self.by_name(name).ok_or_else(|| {
            anyhow::anyhow!(
                "unregistered harness `{name}`; registered harnesses: {}",
                self.harnesses
                    .iter()
                    .map(|harness| harness.id().as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }

    /// Model inference is delegated to the registered adapters. Explicit
    /// caller/config choices are resolved before reaching this method.
    pub fn for_model(&self, model: &str) -> Option<HarnessId> {
        let spec: boop_store::session::ModelSpec = model.parse().ok()?;
        let name = spec.name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return None;
        }
        self.harnesses
            .iter()
            .rev()
            .find(|harness| harness.matches_model(&name))
            .map(|harness| harness.id())
    }

    /// Every session boop-harness can see for one harness, filtered to `cwd`
    /// when one is given. Nothing here parses a transcript.
    pub fn sessions_in_cwd(&self, id: HarnessId, cwd: Option<&str>) -> Vec<SessionRef> {
        let harness = self.get(id);
        harness
            .sessions()
            .unwrap_or_default()
            .into_iter()
            .filter(|session| cwd.is_none_or(|wanted| session.cwd.as_deref() == Some(wanted)))
            .filter(|session| harness.lists_session(session))
            .collect()
    }

    /// Every session for one harness, shaped and sorted newest first (then id
    /// ascending).
    pub fn describe_all(&self, id: HarnessId, cwd: Option<&str>) -> Vec<SessionMeta> {
        let mut sessions: Vec<SessionMeta> = self
            .sessions_in_cwd(id, cwd)
            .iter()
            .filter_map(|session| self.get(id).describe(session))
            .collect();
        sessions.sort_by(|a, b| {
            b.last_activity_ms
                .cmp(&a.last_activity_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions
    }

    /// Newest-first resumable ids for a cwd. No transcript is parsed: the order
    /// is the mtime boop-harness already stated for each session.
    pub fn session_ids_for_cwd(&self, id: HarnessId, cwd: &str) -> Vec<String> {
        let mut found: Vec<(u64, String)> = self
            .sessions_in_cwd(id, Some(cwd))
            .iter()
            .map(|session| {
                (
                    session.modified_ms,
                    self.get(id).resume_id(session).to_string(),
                )
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        found.into_iter().map(|(_, id)| id).collect()
    }

    /// Every turn in one session, resolved by id. `after_seq` returns only
    /// newer turns (the watcher's incremental read).
    pub fn messages_by_id(
        &self,
        id: HarnessId,
        session_id: &str,
        cwd: &str,
        after_seq: Option<u64>,
    ) -> Vec<Message> {
        let Some(session) = self.get(id).session_by_id(session_id, Some(cwd)) else {
            return Vec::new();
        };
        self.get(id).messages(&session, after_seq)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_invocations_classify_commands_and_preserve_value_tokens() {
        let registry = Registry::discover();
        let cases: &[(&str, &[&str], bool)] = &[
            ("codex", &["-m", "review", "resume", "thread"], true),
            ("codex", &["--", "--help"], true),
            ("codex", &["-c", "key=exec", "fork", "thread"], true),
            ("codex", &["-c", "key=value", "exec", "prompt"], false),
            ("codex", &["queue", "--thread", "thread"], false),
            ("codex", &["resume", "--help"], false),
            ("claude", &["--model", "doctor", "--resume", "thread"], true),
            ("claude", &["--system-prompt", "mcp", "prompt"], true),
            ("claude", &["-p", "prompt"], false),
            ("claude", &["--background"], false),
            ("kimi", &["--model", "export", "--continue"], true),
            ("kimi", &["-pprompt"], false),
            ("kimi", &["acp"], false),
            (
                "opencode",
                &["--model", "run", "attach", "http://localhost"],
                true,
            ),
            ("opencode", &["run", "prompt"], false),
        ];
        let observed = cases
            .iter()
            .map(|(id, args, _)| {
                registry
                    .by_name(id)
                    .unwrap()
                    .uses_native_tui(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            cases
                .iter()
                .map(|(_, _, expected)| *expected)
                .collect::<Vec<_>>()
        );
    }

    /// RECEIPT (field, 2026-08-10). `--model gpt-5.6-luna@medium` with no
    /// `--harness` dry-ran as opencode; the spelling names the harness now.
    #[test]
    fn a_model_spelling_names_its_harness() {
        assert_eq!(
            Registry::discover().for_model("gpt-5.6-luna@medium"),
            Some(HarnessId::Codex)
        );
        assert_eq!(
            Registry::discover().for_model("kimi-k2"),
            Some(HarnessId::Kimi)
        );
        assert_eq!(
            Registry::discover().for_model("claude-opus-4"),
            Some(HarnessId::Claude)
        );
        assert_eq!(
            Registry::discover().for_model("openrouter/deepseek/deepseek-v4-flash-0731"),
            Some(HarnessId::Opencode)
        );
        assert_eq!(
            Registry::discover().for_model("zai-coding-plan/glm-4.6"),
            Some(HarnessId::Opencode)
        );
        assert_eq!(Registry::discover().for_model("nothing-known"), None);
        assert_eq!(Registry::discover().for_model(""), None);
    }

    use super::Registry;
    use crate::harness::{
        Capabilities, Harness, HarnessId, LanePolicy, MailPolicy, ReadChunk, SessionRef,
        VariantSupport,
    };

    /// The replace-a-harness drill. `Echo` is a whole harness: one `impl
    /// Harness` and one `static CAPABILITIES`. It registers under the closed
    /// enum's `Kimi` variant, so what it proves is that the shared rails read
    /// the registered impl and nothing else; a fifth harness would add a
    /// variant first.
    struct Echo;

    static CAPABILITIES: Capabilities = Capabilities {
        bans_plan_family_models: true,
        lanes: LanePolicy::CoordinatorSubagentsOnly,
        variant: VariantSupport::Flag,
        mail: MailPolicy::Door,
        image_paste_keys: Some("C-v"),
        native_tui_projector: true,
        wrapper_owns_alternate_screen: false,
    };

    impl Harness for Echo {
        fn id(&self) -> HarnessId {
            HarnessId::Kimi
        }

        fn capabilities(&self) -> &'static Capabilities {
            &CAPABILITIES
        }

        fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
            Ok(Vec::new())
        }

        fn read_from(&self, _session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk> {
            Ok(ReadChunk {
                events: Vec::new(),
                next_offset: offset,
                reset: false,
                skipped: 0,
            })
        }
    }

    /// The spawn rails read declared capabilities, never a harness name; a
    /// registry holding `Echo` answers them from `Echo`'s own static.
    fn spawn_refusals(registry: &Registry, id: HarnessId, plan_family: bool) -> Vec<&'static str> {
        let capabilities = registry.get(id).capabilities();
        let mut refusals = Vec::new();
        if capabilities.bans_plan_family_models && plan_family {
            refusals.push("plan-family model");
        }
        if capabilities.lanes == LanePolicy::CoordinatorSubagentsOnly {
            refusals.push("no lanes");
        }
        refusals
    }

    /// RECEIPT. A harness swapped in under an existing variant changes what
    /// the shared rails do, with nothing else in the tree edited.
    #[test]
    fn a_swapped_in_impl_drives_the_shared_rails_under_its_variant() {
        let registry = Registry::with(vec![Box::new(Echo)]);
        let echo = registry.get(HarnessId::Kimi);
        assert_eq!(echo.id(), HarnessId::Kimi);
        assert_eq!(echo.capabilities().mail, MailPolicy::Door);
        assert_eq!(
            spawn_refusals(&registry, HarnessId::Kimi, true),
            vec!["plan-family model", "no lanes"]
        );
        assert_eq!(
            registry.by_name("kimi").map(|harness| harness.id()),
            Some(HarnessId::Kimi)
        );
        assert!(registry.by_name("nothing-known").is_none());
        assert!(registry.by_name("codex").is_none());
    }

    /// RECEIPT. The built-in registry answers every variant, so `get` never
    /// has a missing case to report.
    #[test]
    fn every_variant_resolves_in_the_built_in_registry() {
        let registry = Registry::discover();
        for id in HarnessId::ALL {
            assert_eq!(registry.get(id).id(), id);
        }
        assert_eq!(registry.all().len(), HarnessId::ALL.len());
    }
}
