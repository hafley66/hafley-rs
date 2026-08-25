use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use boop_store::session::{Effort, ModelSpec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub default_model_preset: Option<String>,
    pub model_presets: BTreeMap<String, PresetEntry>,
    /// Model-name prefix -> harness id, overriding lane.rs's compiled table.
    pub model_harness: BTreeMap<String, String>,
    /// Model-family prefix -> owning harness for the flat-rate-plan ban.
    pub opencode_banned: BTreeMap<String, String>,
}

/// One named preset. `harness`, `model` and `effort` are separate fields: a
/// model spelling does not always name its harness (`kimi-code/k3` reads as a
/// provider path) and an effort belongs in the harness's own config flag.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ModelPreset {
    /// The harness that runs this preset. `None` derives it from the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    pub model: String,
    /// Reasoning effort: codex takes it as `-c model_reasoning_effort=`, never
    /// as an `@suffix` in the model string (presets-only-model-spelling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// opencode's own reasoning-effort spelling, a `--variant` flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// The executable the harness is launched as, replacing its own binary
    /// (`ccz` is claude under the z.ai env). `None` keeps the harness default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
}

impl ModelPreset {
    /// A bare `--model` spelling read as a preset of one row. An `@effort`
    /// suffix is split off here so it never travels inside a model string.
    pub fn from_model(model: &str) -> Result<ModelPreset> {
        ModelPreset {
            model: model.to_owned(),
            ..Default::default()
        }
        .split_effort()
    }

    /// Move an `@effort` suffix out of `model` and into `effort`. An explicit
    /// `effort` field wins; a suffix naming no known effort is an error.
    fn split_effort(mut self) -> Result<ModelPreset> {
        let spec: ModelSpec = self.model.parse()?;
        self.model = spec.name;
        match &self.effort {
            Some(effort) => {
                effort.parse::<Effort>()?;
            }
            None => self.effort = spec.effort.map(|effort| effort.as_str().to_owned()),
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PresetEntry {
    Object(ModelPreset),
    Legacy(String),
}

static CONFIG: OnceLock<Result<Config, anyhow::Error>> = OnceLock::new();

/// The process-wide Config, loaded exactly once from the default path. A
/// missing file falls back to the default (nothing configured); a file that
/// fails to read or parse is a loud error, never a silent default.
pub fn loaded() -> Result<&'static Config> {
    match CONFIG.get_or_init(load_once) {
        Ok(config) => Ok(config),
        Err(error) => Err(anyhow::anyhow!("load the boop config: {error:#}")),
    }
}

fn load_once() -> Result<Config, anyhow::Error> {
    let path = default_path()?;
    load(&path)
}

pub fn default_path() -> Result<PathBuf> {
    let root = dirs::config_dir().context("resolve the user config directory")?;
    Ok(root.join("boop").join("config.json"))
}

pub fn load(path: &Path) -> Result<Config> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn resolve_model(preset: &str, path: &Path) -> Result<String> {
    Ok(resolve_preset(preset, path)?.model)
}

/// The reasoning effort a named preset carries, if any.
pub fn resolve_effort(preset: &str, path: &Path) -> Result<Option<String>> {
    Ok(resolve_preset(preset, path)?.effort)
}

/// The opencode variant a named preset carries, if any. A preset that names
/// no variant resolves to `None`, meaning the CLI flag decides alone.
pub fn resolve_variant(preset: &str, path: &Path) -> Result<Option<String>> {
    Ok(resolve_preset(preset, path)?.variant)
}

/// The full named preset, both the model string and its optional variant.
pub fn resolve_preset(preset: &str, path: &Path) -> Result<ModelPreset> {
    let config = load(path)?;
    config
        .model_presets
        .get(preset)
        .cloned()
        .map(|entry| match entry {
            PresetEntry::Object(preset) => preset,
            PresetEntry::Legacy(model) => ModelPreset {
                model,
                ..Default::default()
            },
        })
        .with_context(|| {
            let available = config.model_presets.keys().cloned().collect::<Vec<_>>();
            let available = if available.is_empty() {
                "none".to_owned()
            } else {
                available.join(", ")
            };
            format!(
                "model preset `{preset}` is absent from {} (available: {available})",
                path.display()
            )
        })?
        .split_effort()
}

/// Pick a lane's spawn model: explicit --model, then --preset, then
/// default-model-preset, applied to every harness. The explicit slot carries
/// an already resolved model string; `preset` and `default_preset` are names
/// resolved to model strings on demand.
pub fn resolve_spawn_model(
    explicit: Option<&str>,
    preset: Option<&str>,
    default_preset: Option<&str>,
    path: &Path,
) -> Result<Option<String>> {
    if let Some(model) = explicit {
        return Ok(Some(model.to_owned()));
    }
    if let Some(preset) = preset {
        return Ok(Some(resolve_model(preset, path)?));
    }
    if let Some(preset) = default_preset {
        return Ok(Some(resolve_model(preset, path)?));
    }
    Ok(None)
}

/// The loaded config as pretty JSON, including the defaults a missing file
/// yields. `boop config show` prints this.
pub fn show(path: &Path) -> Result<String> {
    let config = load(path)?;
    serde_json::to_string_pretty(&config)
        .map_err(|error| anyhow::anyhow!("serialize the boop config: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_provider_model_presets() {
        let config: Config = serde_json::from_str(
            r#"
{
  "default-model-preset": "flash4",
  "model-presets": {
    "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
    "luna": "gpt-5.6-luna@medium"
  },
  "model-harness": {
    "glm": "opencode"
  },
  "opencode-banned": {
    "gemini": "gemini"
  }
}
"#,
        )
        .unwrap();
        assert_eq!(
            config,
            Config {
                default_model_preset: Some("flash4".into()),
                model_presets: BTreeMap::from([
                    (
                        "flash4".into(),
                        PresetEntry::Legacy("openrouter/deepseek/deepseek-v4-flash-0731".into())
                    ),
                    (
                        "luna".into(),
                        PresetEntry::Legacy("gpt-5.6-luna@medium".into())
                    ),
                ]),
                model_harness: BTreeMap::from([("glm".into(), "opencode".into())]),
                opencode_banned: BTreeMap::from([("gemini".into(), "gemini".into())]),
            }
        );
    }

    #[test]
    fn preset_object_form_carries_a_variant() {
        let config: Config = serde_json::from_str(
            r#"
{
  "model-presets": {
    "flash4": { "model": "openrouter/deepseek/deepseek-v4-flash-0731", "variant": "high" },
    "luna": "gpt-5.6-luna@medium"
  }
}
"#,
        )
        .unwrap();
        assert!(matches!(
            config.model_presets.get("flash4"),
            Some(PresetEntry::Object(_))
        ));
        assert!(matches!(
            config.model_presets.get("luna"),
            Some(PresetEntry::Legacy(_))
        ));
        let path = write_config(
            r#"{ "model-presets": {
                "flash4": { "model": "openrouter/deepseek/deepseek-v4-flash-0731", "variant": "high" },
                "luna": "gpt-5.6-luna@medium" } }"#,
            "object-preset",
        );
        assert_eq!(
            resolve_model("flash4", &path).unwrap(),
            "openrouter/deepseek/deepseek-v4-flash-0731"
        );
        assert_eq!(
            resolve_variant("flash4", &path).unwrap(),
            Some("high".into())
        );
        assert_eq!(resolve_variant("luna", &path).unwrap(), None);
    }

    /// A preset object naming an alternate executable survives the round
    /// trip, and the legacy bare-string form still resolves to no executable.
    #[test]
    fn preset_object_form_carries_a_bin() {
        let text = r#"{ "model-presets": {
                "zfable": { "model": "claude-fable-5@high", "bin": "ccz" },
                "fable": "claude-fable-5@high" } }"#;
        let config: Config = serde_json::from_str(text).unwrap();
        let dumped = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<Config>(&dumped).unwrap(),
            config,
            "{dumped}"
        );
        let path = write_config(text, "object-bin");
        let preset = resolve_preset("zfable", &path).unwrap();
        assert_eq!(preset.model, "claude-fable-5");
        assert_eq!(preset.effort.as_deref(), Some("high"));
        assert_eq!(preset.bin.as_deref(), Some("ccz"));
        assert_eq!(resolve_preset("fable", &path).unwrap().bin, None);
    }

    fn write_config(text: &str, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("boop-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn default_preset_resolves_for_any_harness() {
        let path = write_config(
            r#"{ "default-model-preset": "flash4", "model-presets": {
                "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "any-harness",
        );
        assert_eq!(
            resolve_spawn_model(None, None, Some("flash4"), &path).unwrap(),
            Some("openrouter/deepseek/deepseek-v4-flash-0731".into())
        );
    }

    #[test]
    fn explicit_model_beats_preset_beats_default() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "precedence",
        );
        assert_eq!(
            resolve_spawn_model(Some("my-model"), Some("flash4"), Some("luna"), &path).unwrap(),
            Some("my-model".into())
        );
        assert_eq!(
            resolve_spawn_model(None, Some("flash4"), Some("luna"), &path).unwrap(),
            Some("openrouter/deepseek/deepseek-v4-flash-0731".into())
        );
        assert_eq!(
            resolve_spawn_model(None, None, Some("luna"), &path).unwrap(),
            Some("gpt-5.6-luna".into())
        );
        assert_eq!(resolve_spawn_model(None, None, None, &path).unwrap(), None);
    }

    #[test]
    fn collapsed_resolution_matches_all_precedence_cases() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "collapsed",
        );
        let explicit = Some("my-model");
        let preset = Some("flash4");
        let default = Some("luna");
        let cases = [
            (explicit, preset, default, Some("my-model".to_owned())),
            (
                None,
                preset,
                default,
                Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            ),
            (None, None, default, Some("gpt-5.6-luna".to_owned())),
            (None, None, None, None),
        ];
        for (explicit, preset, default, expected) in cases {
            assert_eq!(
                resolve_spawn_model(explicit, preset, default, &path).unwrap(),
                expected
            );
        }
    }

    /// RECEIPT (presets-only-model-spelling). The legacy `name@effort` string
    /// resolves to a bare model and a separate effort, so `gpt-5.6-luna@medium`
    /// never reaches an ACP `model` option again.
    #[test]
    fn a_legacy_at_effort_string_resolves_to_model_plus_effort() {
        let path = write_config(
            r#"{ "model-presets": { "luna": "gpt-5.6-luna@medium" } }"#,
            "legacy-effort",
        );
        let preset = resolve_preset("luna", &path).unwrap();
        assert_eq!(preset.model, "gpt-5.6-luna");
        assert_eq!(preset.effort.as_deref(), Some("medium"));
        assert_eq!(resolve_model("luna", &path).unwrap(), "gpt-5.6-luna");
        assert_eq!(
            resolve_effort("luna", &path).unwrap(),
            Some("medium".into())
        );
    }

    /// A model spelling does not always name its harness: `kimi-code/k3` reads
    /// as a provider path, which would route to opencode.
    #[test]
    fn a_preset_carries_its_harness_and_effort_as_fields() {
        let path = write_config(
            r#"{ "model-presets": {
                "k3": { "harness": "kimi", "model": "kimi-code/k3" },
                "sol": { "harness": "codex", "model": "gpt-5.6-sol", "effort": "high" } } }"#,
            "explicit-fields",
        );
        let k3 = resolve_preset("k3", &path).unwrap();
        assert_eq!(k3.harness.as_deref(), Some("kimi"));
        assert_eq!(k3.model, "kimi-code/k3");
        assert_eq!(k3.effort, None);
        let sol = resolve_preset("sol", &path).unwrap();
        assert_eq!(sol.harness.as_deref(), Some("codex"));
        assert_eq!(sol.effort.as_deref(), Some("high"));
        assert!(!sol.model.contains('@'), "{}", sol.model);
    }

    /// An effort nobody recognizes is a config error, named at resolve time.
    #[test]
    fn an_unknown_effort_is_refused_by_name() {
        let path = write_config(
            r#"{ "model-presets": { "bad": { "model": "gpt-5.6-sol", "effort": "turbo" } } }"#,
            "bad-effort",
        );
        let error = resolve_preset("bad", &path).unwrap_err().to_string();
        assert!(error.contains("turbo"), "{error}");
    }

    #[test]
    fn missing_preset_lists_available_names() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "missing-preset",
        );
        let error = resolve_model("nope", &path).unwrap_err().to_string();
        assert!(
            error.contains("model preset `nope` is absent from"),
            "{error}"
        );
        assert!(error.contains("available: flash4, luna"), "{error}");
    }

    #[test]
    fn show_on_missing_file_prints_the_default_config() {
        let dir = std::env::temp_dir().join(format!("boop-config-show-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let rendered = show(&path).unwrap();
        let parsed: Config = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed, Config::default());
        assert!(rendered.contains("\"model-presets\": {}"), "{rendered}");
    }
}
