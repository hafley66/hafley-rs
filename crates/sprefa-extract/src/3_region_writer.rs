//! `extract region`: check or apply one generated comment region through Soopy.

use crate::cli::RegionArgs;
use std::io::Read;
use std::path::Path;

use sprefa_extract::move_stage::{stage_and_commit, state_root};
use sprefa_extract::propose_owned_region;

pub fn run(cli: RegionArgs) -> crate::RyiResult<i32> {
    let target = sprefa_extract::io_path(&cli.target)
        .canonicalize()
        .map_err(|error| {
            crate::RyiExit::new(2, format!("open target {}: {error}", cli.target.display()))
        })?;
    let before = std::fs::read(&target).map_err(|error| {
        crate::RyiExit::new(2, format!("read target {}: {error}", target.display()))
    })?;
    let generated = read_generated(&cli.generated)?;
    let proposal = propose_owned_region(&before, &cli.id, &generated)
        .map_err(|error| crate::RyiExit::new(2, format!("region {}: {error}", cli.id)))?;
    if !proposal.changed() {
        print_status(
            "current",
            &proposal.region.id,
            proposal.region.start,
            proposal.region.end,
            None,
        );
        return Ok(0);
    }
    if !cli.apply {
        print_status(
            "drift",
            &proposal.region.id,
            proposal.region.start,
            proposal.region.end,
            None,
        );
        return Ok(1);
    }

    let root = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            crate::RyiExit::new(
                2,
                format!("target has no UTF-8 file name: {}", target.display()),
            )
        })?;
    let source_root = soopy::SourceRoot::open_directory(root).map_err(|error| {
        crate::RyiExit::new(2, format!("open target root {}: {error}", root.display()))
    })?;
    let directory = source_root.directory().identity.clone();
    let source = soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory: directory.clone(),
            path: soopy::RootPath(name.into()),
        },
    };
    let request = proposal.stage_request(
        soopy::SourceRootId::Directory { directory },
        source,
        soopy::ActionProducer::unordered("dl7-owned-region"),
    );
    let state =
        state_root(cli.state.as_deref()).map_err(|message| crate::RyiExit::new(2, message))?;
    let (stage, _) = stage_and_commit(root, &state, &request.actions, soopy::Durability::Durable)
        .map_err(|message| crate::RyiExit::new(2, message))?;
    print_status(
        "applied",
        &proposal.region.id,
        proposal.region.start,
        proposal.region.end,
        Some(&stage),
    );
    Ok(0)
}

fn read_generated(path: &Path) -> crate::RyiResult<String> {
    if path == Path::new("-") {
        let read = match crate::ops::request_input_file() {
            Some(input) => std::fs::read_to_string(input.path()),
            None => {
                let mut generated = String::new();
                std::io::stdin()
                    .read_to_string(&mut generated)
                    .map(|_| generated)
            }
        };
        read.map_err(|error| crate::RyiExit::new(2, format!("read generated stdin: {error}")))
    } else {
        std::fs::read_to_string(sprefa_extract::io_path(path)).map_err(|error| {
            crate::RyiExit::new(2, format!("read generated {}: {error}", path.display()))
        })
    }
}

fn print_status(status: &str, region: &str, start: u64, end: u64, stage: Option<&str>) {
    crate::outln!(
        "{}",
        serde_json::json!({
            "status": status,
            "region": region,
            "start": start,
            "end": end,
            "stage": stage,
        })
    );
}
