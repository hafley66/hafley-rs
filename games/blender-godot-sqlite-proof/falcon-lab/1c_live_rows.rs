//! Latest-only local IPC: bincode preserves f64 row bits; same-directory rename publishes atomically.
use crate::fixture::sql_viewer::boundary::ROW_CAPACITY;
pub(crate) use crate::fixture::sql_viewer::boundary::contracts::Latest;
use crate::fixture::sql_viewer::boundary::contracts::{IPC_LIMIT, PROTOCOL_VERSION};
use std::path::Path;

const LIMIT: usize = IPC_LIMIT as usize;

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
    assert_eq!(latest.version, PROTOCOL_VERSION);
    assert!(!latest.rows.is_empty() && latest.rows.len() <= ROW_CAPACITY);
    assert!(latest.rows.iter().all(|r| r.tick == latest.rows[0].tick));
    assert_eq!(latest.rows[0].kind, 0);
    Ok(latest)
}

#[cfg(test)]
mod tests {
    use crate::fixture::sql_viewer::boundary::Row;

    #[test]
    fn generated_envelope_matches_v1_wire_fixture() {
        use super::*;
        let mut row = Row::new(0, 0, 0);
        row.values[0] = f64::from_bits(0x3ff0000000000001);
        let latest = Latest { version: PROTOCOL_VERSION, pid: 1, generation: 3, elapsed_us: 0, rows: vec![row] };
        // Frozen v1 bincode layout: five envelope fields, three row integers,
        // then 24 little-endian f64 values. No added array-length prefix.
        let mut fixture = vec![1, 1, 3, 0, 1, 0, 0, 0];
        fixture.extend_from_slice(&[1, 0, 0, 0, 0, 0, 240, 63]);
        fixture.resize(8 + 24 * 8, 0);
        assert_eq!(bincode::serde::encode_to_vec(&latest, bincode::config::standard()).unwrap(), fixture);
        let (decoded, used): (Latest, usize) = bincode::serde::decode_from_slice(&fixture, bincode::config::standard()).unwrap();
        assert_eq!((decoded, used), (latest, fixture.len()));
    }

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
                    version: PROTOCOL_VERSION,
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
