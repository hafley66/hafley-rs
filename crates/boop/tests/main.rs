//! Every file under tests/ is a module of this one target. Selecting one
//! file's tests is `--test main -- <module>`, and `autotests = false` in
//! Cargo.toml is what stops cargo minting a second target per file.

#[path = "boop_start_warm.rs"]
mod boop_start_warm;
#[cfg(feature = "dl6")]
#[path = "concatmap_e2e.rs"]
mod concatmap_e2e;
#[path = "coordinator_ping.rs"]
mod coordinator_ping;
#[path = "deliver_door.rs"]
mod deliver_door;
#[cfg(feature = "dl6")]
#[path = "host_chat.rs"]
mod host_chat;
#[path = "inbox_hooks.rs"]
mod inbox_hooks;
#[path = "install_rail.rs"]
mod install_rail;
#[path = "lane_carcass.rs"]
mod lane_carcass;
#[path = "lane_completion_row.rs"]
mod lane_completion_row;
#[path = "lane_debug.rs"]
mod lane_debug;
mod lane_retire_revive;
#[path = "lane_wait_exit.rs"]
mod lane_wait_exit;
#[path = "native_agent_liveness.rs"]
mod native_agent_liveness;
#[path = "native_projector_contention.rs"]
mod native_projector_contention;
#[path = "no_sync_hatch.rs"]
mod no_sync_hatch;
#[path = "preset_dry_run.rs"]
mod preset_dry_run;
#[path = "registry_kinds.rs"]
mod registry_kinds;
#[path = "session_mood.rs"]
mod session_mood;
#[path = "sync_convoy.rs"]
mod sync_convoy;
#[path = "sync_discovery.rs"]
mod sync_discovery;
#[path = "0_sqlite_contention.rs"]
mod t0_sqlite_contention;
#[path = "tell.rs"]
mod tell;
#[path = "temp_home_rail.rs"]
mod temp_home_rail;
#[path = "wait_mail.rs"]
mod wait_mail;
