use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sprefa_lab_bakeoff::{score, CaseAnswer, Cell, TOOLS};

/// Lab root is the crate dir, so the binary reads the same `expected/` and
/// `out/` trees from any cwd.
fn lab_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load(path: &Path) -> Result<CaseAnswer, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn case_ids(expected_dir: &Path) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    let entries =
        fs::read_dir(expected_dir).map_err(|e| format!("{}: {e}", expected_dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    ids.sort();
    Ok(ids)
}

fn print_table(header: &[String], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = header.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }
    let line = |cells: &[String]| {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c:<width$}", width = widths[i]))
            .collect();
        println!("| {} |", padded.join(" | "));
    };
    line(header);
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    line(&rule);
    for row in rows {
        line(row);
    }
}

fn main() -> ExitCode {
    let root = lab_root();
    let expected_dir = root.join("expected");
    let ids = match case_ids(&expected_dir) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("score: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut header = vec!["case".to_string()];
    header.extend(TOOLS.iter().map(|t| t.to_string()));
    let mut rows = Vec::new();
    let mut details = Vec::new();
    let mut failed = false;

    for id in &ids {
        let expected: BTreeSet<String> = match load(&expected_dir.join(format!("{id}.json"))) {
            Ok(answer) => answer.set(),
            Err(e) => {
                eprintln!("score: {e}");
                failed = true;
                continue;
            }
        };
        let mut row = vec![id.clone()];
        for tool in TOOLS {
            let out_file = root.join("out").join(tool).join(format!("{id}.json"));
            let cell = if !out_file.exists() {
                Cell::Missing
            } else {
                match load(&out_file) {
                    Ok(got) => score(&expected, &got),
                    Err(e) => {
                        eprintln!("score: {e}");
                        failed = true;
                        continue;
                    }
                }
            };
            if let Cell::Diff { extra, missing } = &cell {
                for entry in extra {
                    details.push(format!("{id} {tool} +{entry}"));
                }
                for entry in missing {
                    details.push(format!("{id} {tool} -{entry}"));
                }
            }
            row.push(cell.render());
        }
        rows.push(row);
    }

    print_table(&header, &rows);
    if !details.is_empty() {
        println!();
        println!("diff detail:");
        for detail in &details {
            println!("  {detail}");
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
