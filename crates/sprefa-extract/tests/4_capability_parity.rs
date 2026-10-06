#[test]
fn whole_output() {
    crate::fixture_runner::run("capability_parity", |case| {
        crate::fixture_runner::commands(case, |step| {
            if step["api"] == "declared_capabilities" {
                assert_eq!(ALL.len(), DECLARED_CAPABILITIES);
                serde_json::json!(ALL.iter().map(|capability| capability.name()).collect::<Vec<_>>())
            } else { crate::fixture_runner::capability_api(step) }
        })
    });
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LibraryCapability {
    /// `flatten` / `flatten_jsonl` over a dispatched file.
    Phase1Flatten,
    /// `FamilyMask` selection of a family subset.
    Phase1FamilyMask,
    /// `Resolve<CallF>`: resolved caller-to-callee edges.
    ResolveCall,
    /// `Resolve<TypeF>`: resolved type reference edges.
    ResolveType,
    /// `ScipSource::load` plus a reader placed in `ProjectCx`, which is what
    /// turns on the resolve arms' SCIP leg.
    ScipIndexLoad,
    /// `ScipSource::build`: run the language's own indexer.
    ScipIndexBuild,
    /// `wire::SCHEMA`: the JSONL contract text.
    WireSchema,
    /// `wire::flatten_project_type`: the span-addressed project-edge wire.
    FlattenProjectType,
    /// `wire::flatten_scip` / `project::scip_facts`: the loaded SCIP index as
    /// raw occurrence, symbol and relationship rows.
    ScipFacts,
    /// `wire::file_fact`: one file's digest, byte count and line count.
    FileFact,
    /// `wire::scip_file_edges`: the module graph folded out of a SCIP index.
    ScipFileEdges,
    /// `deps::fold_edges`: the same module graph resolved syntactically.
    DietFileEdges,
    /// `deps::fold_unresolved`: the specifiers that resolved to nothing.
    DietFileUnresolved,
    /// `manifests::fold_package_edges`: workspace-internal manifest edges.
    PackageEdges,
    /// `slow::slow_project`: a SCIP index projected onto fast's tables.
    SlowProject,
}

/// Kept in step with the enum by the length assertion in the test below.
const ALL: &[LibraryCapability] = &[
    LibraryCapability::Phase1Flatten,
    LibraryCapability::Phase1FamilyMask,
    LibraryCapability::ResolveCall,
    LibraryCapability::ResolveType,
    LibraryCapability::ScipIndexLoad,
    LibraryCapability::ScipIndexBuild,
    LibraryCapability::WireSchema,
    LibraryCapability::FlattenProjectType,
    LibraryCapability::ScipFacts,
    LibraryCapability::FileFact,
    LibraryCapability::ScipFileEdges,
    LibraryCapability::DietFileEdges,
    LibraryCapability::DietFileUnresolved,
    LibraryCapability::PackageEdges,
    LibraryCapability::SlowProject,
];
const DECLARED_CAPABILITIES: usize = 15;


impl LibraryCapability {
    fn name(self) -> &'static str {
        match self {
            Self::Phase1Flatten => "Phase1Flatten",
            Self::Phase1FamilyMask => "Phase1FamilyMask",
            Self::ResolveCall => "ResolveCall",
            Self::ResolveType => "ResolveType",
            Self::ScipIndexLoad => "ScipIndexLoad",
            Self::ScipIndexBuild => "ScipIndexBuild",
            Self::WireSchema => "WireSchema",
            Self::FlattenProjectType => "FlattenProjectType",
            Self::ScipFacts => "ScipFacts",
            Self::FileFact => "FileFact",
            Self::ScipFileEdges => "ScipFileEdges",
            Self::DietFileEdges => "DietFileEdges",
            Self::DietFileUnresolved => "DietFileUnresolved",
            Self::PackageEdges => "PackageEdges",
            Self::SlowProject => "SlowProject",
        }
    }
}
