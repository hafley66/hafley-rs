pub mod fighters {
    #[path = "falcon/2_simulation.rs"]
    pub mod falcon;
    #[cfg(feature = "ingest")]
    #[path = "falcon/1b_catalog.rs"]
    pub mod falcon_catalog;
}
