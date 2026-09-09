//! `boop config presets` machine shape: `--format json` prints the preset rows
//! as a JSON array with the same fields and order the fixed-width table prints,
//! so instant's context menu can group presets by harness without parsing the
//! table.

use boop_store::testing::BoopCommandExt;
use std::path::PathBuf;
use std::process::Command;

const TABLE: &str = include_str!("fixtures/preset_table.json");

/// Byte-identical snapshot of today's `boop config presets` table for this
/// fixture. Any change to the table renderer breaks this receipt on purpose.
const TABLE_OUTPUT: &str = "PRESET      HARNESS   MODEL                                       EFFORT  VARIANT  BIN  STATUS\nfable       claude    claude-fable-5                              high                  ok\nfable-max   claude    claude-fable-5                              max                   ok\nflash4      opencode  openrouter/deepseek/deepseek-v4-flash-0731                        ok *\nflash4-max  opencode  openrouter/deepseek/deepseek-v4-flash-0731          xhigh         ok\ngem37       opencode  openrouter/google/gemini-3.7-flash                                ok\nglm53       claude    fable                                       high             ccz  ok\nhaiku       claude    claude-haiku-4-5-20251001                   low                   ok\nk3          kimi      kimi-code/k3                                                      ok\nk3-256k     kimi      kimi-code/k3-256k                                                 ok\nkimi        kimi      kimi-code/kimi-for-coding                                         ok\nkimi-fast   kimi      kimi-code/kimi-for-coding-highspeed                               ok\nluna        codex     gpt-5.6-luna                                medium                ok\nopus        claude    claude-opus-5                               high                  ok\nopus-max    claude    claude-opus-5                               max                   ok\nox          opencode  openrouter/stealth/ox-alpha                                       ok\npro4        opencode  openrouter/deepseek/deepseek-v4-pro-0813                          ok\npro4-max    opencode  openrouter/deepseek/deepseek-v4-pro-0813            xhigh         ok\nq38         opencode  openrouter/qwen/qwen3.8-27b                                       ok\nsol         codex     gpt-5.6-sol                                 high                  ok\nsol-xhigh   codex     gpt-5.6-sol                                 xhigh                 ok\nsolx        opencode  openrouter/openai/gpt-5.6-sol                                     DEAD model `openrouter/openai/gpt-5.6-sol` is BANNED from opencode: its family runs on the `codex` harness's flat-rate plan, and opencode would pay metered API credit for it. Spell the bare model name (no provider path) so the `codex` harness picks it up.\nsonnet      claude    claude-sonnet-5                             medium                ok\nterra       codex     gpt-5.6-terra                               medium                ok\nzfable      claude    fable                                       high             ccz  ok\nzopus       claude    opus                                        high             ccz  ok\nzsonnet     claude    sonnet                                      medium           ccz  ok\n";

const KEYS: [&str; 8] = [
    "name", "harness", "model", "effort", "variant", "bin", "status", "default",
];

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let root = std::env::temp_dir().join(format!("boop-presets-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // `dirs::config_dir` reads HOME on macOS and XDG_CONFIG_HOME on linux;
        // the config is written where each one looks.
        for config in [
            root.join("Library/Application Support/boop"),
            root.join("config/boop"),
        ] {
            std::fs::create_dir_all(&config).unwrap();
            std::fs::write(config.join("config.json"), TABLE).unwrap();
        }
        Fixture { root }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_boop"))
            .boop_test_root(&self.root)
            .env("BOOP_CONFIG", self.root.join("config/boop/config.json"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(args)
            .output()
            .expect("run the boop binary")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// RECEIPT. `--format json` prints a JSON array whose every element carries the
/// eight preset fields, in the same config-file order the table prints.
#[test]
fn json_format_is_a_full_array_of_preset_rows() {
    let fixture = Fixture::new();
    let output = fixture.run(&["config", "presets", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output is a JSON value");
    let rows = value.as_array().expect("output is a JSON array");
    let preset_count = serde_json::from_str::<serde_json::Value>(TABLE).unwrap()["model-presets"]
        .as_object()
        .unwrap()
        .len();
    assert_eq!(rows.len(), preset_count, "one row per preset");
    for row in rows {
        let obj = row
            .as_object()
            .unwrap_or_else(|| panic!("row not an object: {row}"));
        for key in KEYS {
            assert!(obj.contains_key(key), "row {row} lacks key {key}");
        }
    }
    let names: Vec<&str> = rows
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    let table_names: Vec<&str> = TABLE_OUTPUT
        .lines()
        .skip(1)
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(
        names, table_names,
        "JSON and table print rows in the same order"
    );
    let flash4 = &rows[names.iter().position(|n| *n == "flash4").unwrap()];
    assert_eq!(flash4["default"], true, "flash4 is the default preset");
}

/// RECEIPT. The table printer is unchanged: `--format table` (the default)
/// still prints byte-for-byte what it did before this feature.
#[test]
fn table_output_is_byte_identical_to_the_snapshot() {
    let fixture = Fixture::new();
    let output = fixture.run(&["config", "presets"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        TABLE_OUTPUT,
        "table renderer changed; update the snapshot only if the change is intended"
    );
}
