//! Latest-only local IPC: bincode preserves f64 row bits; same-directory rename publishes atomically.
use crate::fixture::sql_viewer::boundary::{ROW_CAPACITY, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub(crate) struct Latest {
    pub version: u32,
    pub pid: u32,
    pub generation: u64,
    pub elapsed_us: u64,
    pub rows: Vec<Row>,
}
const LIMIT: usize = 262144;

#[tracing::instrument(target="falcon::ipc", level="trace", skip_all, fields(generation=latest.generation, rows=latest.rows.len()))]
pub(crate) fn publish(path: &Path, latest: &Latest) -> Result<(), Box<dyn std::error::Error>> {
    assert!(!latest.rows.is_empty() && latest.rows.len() <= ROW_CAPACITY);
    let bytes = bincode::serde::encode_to_vec(latest, bincode::config::standard())?;
    assert!(bytes.len() <= LIMIT);
    let pending = path.with_extension("pending");
    std::fs::write(&pending, bytes)?;
    std::fs::rename(pending, path)?;
    Ok(())
}

#[tracing::instrument(target = "falcon::ipc", level = "trace", skip_all)]
pub(crate) fn read(path: &Path) -> Result<Latest, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    assert!(bytes.len() <= LIMIT);
    let (latest, used): (Latest, usize) = bincode::serde::decode_from_slice(
        &bytes,
        bincode::config::standard().with_limit::<LIMIT>(),
    )?;
    assert_eq!(used, bytes.len());
    assert_eq!(latest.version, 1);
    assert!(!latest.rows.is_empty() && latest.rows.len() <= ROW_CAPACITY);
    assert!(latest.rows.iter().all(|r| r.tick == latest.rows[0].tick));
    assert_eq!(latest.rows[0].kind, 0);
    Ok(latest)
}

#[cfg(test)]
mod tests {
    #[test]
    fn latest_replaces_without_backlog_and_preserves_float_bits() {
        use super::*;
        let dir = std::env::temp_dir().join(format!("falcon-live-test-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("latest.bin");
        let mut row = Row::new(0, 0, 0);
        row.values[0] = f64::from_bits(0x3ff0000000000001);
        for generation in 1..=3 {
            publish(
                &path,
                &Latest {
                    version: 1,
                    pid: 1,
                    generation,
                    elapsed_us: 0,
                    rows: vec![row],
                },
            )
            .unwrap();
        }
        let latest = read(&path).unwrap();
        assert_eq!(latest.generation, 3);
        assert_eq!(latest.rows[0].values[0].to_bits(), row.values[0].to_bits());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
