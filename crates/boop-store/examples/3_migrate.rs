//! Explicit-path migration driver for scratch rehearsals. No default store lookup.
fn main() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("database path required"))?,
    );
    anyhow::ensure!(path.is_file(), "database must already exist");
    let store = boop_store::Store::open(path)?;
    println!("{}", store.schema_version()?);
    Ok(())
}
