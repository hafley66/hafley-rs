#![cfg(feature = "rust-checker")]
use hafley_scm::read::lang::{rust_module_facts::rust_module_facts, rust_names_index::RustNamesIndex};
use hafley_scm::read::types::{build_def_index, content_id_of};
use hafley_scm::read::{dispatch, FamilyMask};
use std::path::Path;

#[test]
fn names_index_joins_provider_coordinates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rust_names")
        .canonicalize()
        .unwrap();
    let mut files = Vec::new();
    let mut outputs = Vec::new();
    let mut corpus = Vec::new();
    for file in ["0_root.rs", "1_shared.rs", "2_bin.rs"] {
        let path = root.join(file).to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let blob = content_id_of(&bytes);
        files.push((path.clone(), rust_module_facts(&path, &bytes).unwrap()));
        corpus.push((path.clone(), blob.clone()));
        outputs.push((blob, dispatch(&path, &bytes, FamilyMask::ALL).unwrap()));
    }
    let pairs = outputs
        .iter()
        .map(|(blob, output)| (blob.clone(), output.as_ref()))
        .collect::<Vec<_>>();
    let defs = build_def_index(&pairs);
    let index = RustNamesIndex::build(files, &corpus, &defs);
    let mut output = Vec::new();
    for (file, qualifier, name) in [
        ("0_root.rs", "", "renamed"),
        ("2_bin.rs", "names_fixture", "renamed"),
        ("2_bin.rs", "crate::included", "target"),
    ] {
        let path = root.join(file).to_string_lossy().to_string();
        let qualifier = qualifier
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let target = index
            .qualified_binding(&path, &qualifier, name)
            .unwrap()
            .unwrap();
        output.push((
            file,
            target
                .target_path
                .strip_prefix(root.to_str().unwrap())
                .unwrap()
                .trim_start_matches('/')
                .to_string(),
            target.target_name,
        ));
    }
    assert_eq!(
        output,
        vec![
            ("0_root.rs", "1_shared.rs".into(), Some("target".into())),
            ("2_bin.rs", "1_shared.rs".into(), Some("target".into())),
            ("2_bin.rs", "1_shared.rs".into(), Some("target".into()))
        ]
    );
    let shared = root.join("1_shared.rs").to_string_lossy().to_string();
    let source = std::fs::read_to_string(&shared).unwrap();
    let mut associated = Vec::new();
    for (written, offset, expected) in [
        ("crate::shared::Item::new", None, "new"),
        (
            "Self::Variant",
            Some(source.find("Self::Variant").unwrap() as u32),
            "Variant",
        ),
        (
            "Item::make",
            Some(source.find("Self::helper").unwrap() as u32),
            "make",
        ),
        ("crate::shared::Alias::new", None, "new"),
        ("crate::shared::Kind::Variant", None, "Variant"),
        (
            "Self::helper",
            Some(source.find("Self::helper").unwrap() as u32),
            "helper",
        ),
    ] {
        let file = if offset.is_some() {
            &shared
        } else {
            &root.join("0_root.rs").to_string_lossy().to_string()
        };
        let resolved = index.binding_at(
            file,
            &written.split("::").map(str::to_string).collect::<Vec<_>>(),
            offset,
            hafley_scm::read::shape::FamilyTag::Call,
        );
        associated.push((
            written,
            resolved.ok().and_then(|bound| bound.target_name),
            expected,
        ));
    }
    assert_eq!(
        associated,
        vec![
            ("crate::shared::Item::new", Some("new".into()), "new"),
            ("Self::Variant", Some("Variant".into()), "Variant"),
            ("Item::make", Some("make".into()), "make"),
            ("crate::shared::Alias::new", Some("new".into()), "new"),
            (
                "crate::shared::Kind::Variant",
                Some("Variant".into()),
                "Variant"
            ),
            ("Self::helper", Some("helper".into()), "helper"),
        ]
    );
    assert_eq!(index.context_failures, vec![]);
    let root_file = root.join("0_root.rs").to_string_lossy().to_string();
    let local = index
        .bindings(&root_file)
        .into_iter()
        .filter(|row| row.local == "local")
        .map(|row| {
            (
                row.target_path
                    .strip_prefix(root.to_str().unwrap())
                    .unwrap()
                    .to_string(),
                row.target_name,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(local, vec![("/1_shared.rs".into(), Some("target".into()))]);
    let file = root.join("0_root.rs").to_string_lossy().to_string();
    let text = std::fs::read_to_string(&file).unwrap();
    let offset = text.find("local();").unwrap() as u32;
    assert_eq!(
        index
            .binding_at(
                &file,
                &["local".into()],
                Some(offset),
                hafley_scm::read::shape::FamilyTag::Call
            )
            .unwrap()
            .target_name,
        Some("target".into())
    );
}
