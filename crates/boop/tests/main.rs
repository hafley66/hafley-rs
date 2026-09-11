//! Every file under tests/ is a module of this one target. Selecting one
//! file's tests is `--test main -- <module>`, and `autotests = false` in
//! Cargo.toml is what stops cargo minting a second target per file.

#[path = "boop_start_warm.rs"]
mod boop_start_warm;
#[path = "codex_usage_e2e.rs"]
mod codex_usage_e2e;
#[path = "commit_push_e2e.rs"]
mod commit_push_e2e;
#[path = "coordinator_ping.rs"]
mod coordinator_ping;
#[path = "deliver_door.rs"]
mod deliver_door;
#[path = "door_backoff_e2e.rs"]
mod door_backoff_e2e;
#[path = "inbox_hooks.rs"]
mod inbox_hooks;
#[path = "install_rail.rs"]
mod install_rail;
#[path = "lane_carcass.rs"]
mod lane_carcass;
#[path = "lane_completion_row.rs"]
mod lane_completion_row;
#[path = "lane_create_env.rs"]
mod lane_create_env;
#[path = "lane_debug.rs"]
mod lane_debug;
mod lane_retire_revive;
#[path = "lane_spawn_identity.rs"]
mod lane_spawn_identity;
#[path = "lane_wait_exit.rs"]
mod lane_wait_exit;
#[path = "native_agent_liveness.rs"]
mod native_agent_liveness;
#[path = "native_projector_contention.rs"]
mod native_projector_contention;
#[path = "no_sync_hatch.rs"]
mod no_sync_hatch;
#[path = "pr_push_e2e.rs"]
mod pr_push_e2e;
#[path = "preset_dry_run.rs"]
mod preset_dry_run;
#[path = "presets_json.rs"]
mod presets_json;
#[path = "registry_kinds.rs"]
mod registry_kinds;
#[path = "session_mood.rs"]
mod session_mood;
#[path = "shout_interrupt.rs"]
mod shout_interrupt;
#[path = "sync_convoy.rs"]
mod sync_convoy;
#[path = "sync_discovery.rs"]
mod sync_discovery;
#[path = "0_sqlite_contention.rs"]
mod t0_sqlite_contention;
#[path = "1_harness_boundaries.rs"]
mod t1_harness_boundaries;
#[path = "2_native_wrapper.rs"]
mod t2_native_wrapper;
#[path = "2_replay_fixture.rs"]
mod t2_replay_fixture;
#[path = "3_terminal_pipe_replay.rs"]
mod t3_terminal_pipe_replay;
#[path = "4_adapter_replay.rs"]
mod t4_adapter_replay;
#[path = "4_lifecycle_gate.rs"]
mod t4_lifecycle_gate;
#[path = "5_delayed_worker_delivery.rs"]
mod t5_delayed_worker_delivery;
#[path = "5_live_harness.rs"]
mod t5_live_harness;
#[path = "tell.rs"]
mod tell;
#[path = "temp_home_rail.rs"]
mod temp_home_rail;
#[path = "tui_sigint_e2e.rs"]
mod tui_sigint_e2e;
#[path = "wait_mail.rs"]
mod wait_mail;
