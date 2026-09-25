//! `extract region`: check or apply one generated comment region through Soopy.

use crate::cli::RegionArgs;
use std::io::Read;
use std::path::Path;

use sprefa_extract::move_stage::{stage_and_commit, state_root};
use sprefa_extract::propose_owned_region;

pub struct RegionError {
    pub message: String,
    pub exit: i32,
}

pub fn run(cli: RegionArgs) -> Result<i32, RegionError> {
    let target = cli.target.canonicalize().map_err(|error| RegionError {
        message: format!("open target {}: {error}", cli.target.display()),
        exit: 2,
    })?;
    let before = std::fs::read(&target).map_err(|error| RegionError {
        message: format!("read target {}: {error}", target.display()),
        exit: 2,
    })?;
    let generated = read_generated(&cli.generated)?;
    let proposal =
        propose_owned_region(&before, &cli.id, &generated).map_err(|error| RegionError {
            message: format!("region {}: {error}", cli.id),
            exit: 2,
        })?;
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
        .ok_or_else(|| RegionError {
            message: format!("target has no UTF-8 file name: {}", target.display()),
            exit: 2,
        })?;
    let source_root = soopy::SourceRoot::open_directory(root).map_err(|error| RegionError {
        message: format!("open target root {}: {error}", root.display()),
        exit: 2,
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
        state_root(cli.state.as_deref()).map_err(|message| RegionError { message, exit: 2 })?;
    let (stage, _) = stage_and_commit(root, &state, &request.actions, soopy::Durability::Durable)
        .map_err(|message| RegionError { message, exit: 2 })?;
    print_status(
        "applied",
        &proposal.region.id,
        proposal.region.start,
        proposal.region.end,
        Some(&stage),
    );
    Ok(0)
}

fn read_generated(path: &Path) -> Result<String, RegionError> {
    if path == Path::new("-") {
        let mut generated = String::new();
        std::io::stdin()
            .read_to_string(&mut generated)
            .map_err(|error| RegionError {
                message: format!("read generated stdin: {error}"),
                exit: 2,
            })?;
        Ok(generated)
    } else {
        std::fs::read_to_string(path).map_err(|error| RegionError {
            message: format!("read generated {}: {error}", path.display()),
            exit: 2,
        })
    }
}

fn print_status(status: &str, region: &str, start: u64, end: u64, stage: Option<&str>) {
    println!(
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
