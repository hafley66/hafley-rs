use std::io::Write;

use serde_json::{json, Value};
use sprefa_extract::{
    rehomes, renames, sources, CheckerTier, FamilyMask, CHECKER_TIERS, INDEXERS, RESOLVE_ARMS,
};

fn planes(mask: FamilyMask) -> Vec<&'static str> {
    [
        (mask.cst, "cst"),
        (mask.types, "types"),
        (mask.call, "call"),
        (mask.df, "df"),
        (mask.data, "data"),
    ]
    .into_iter()
    .filter_map(|(enabled, name)| enabled.then_some(name))
    .collect()
}

fn checker_for(language: &str) -> Option<&'static CheckerTier> {
    CHECKER_TIERS.iter().find(|tier| tier.language == language)
}

fn indexer_language(language: &str) -> &str {
    match language {
        "typescript" => "ts",
        "kotlin/java" => "kotlin",
        other => other,
    }
}

pub fn rows() -> Vec<Value> {
    let mut rows: Vec<Value> = sources()
        .iter()
        .map(|source| {
            let language = source.name();
            let resolve = RESOLVE_ARMS.iter().find(|arm| arm.name == language);
            let rehome = rehomes().iter().find(|arm| arm.name() == language);
            let rename = renames().iter().any(|arm| arm.name() == language);
            let checker = checker_for(language);
            let indexer = INDEXERS
                .iter()
                .find(|indexer| indexer_language(indexer.lang) == language);
            json!({
                "language": language,
                "source": true,
                "planes": planes(source.planes()),
                "resolve": {
                    "call": resolve.is_some_and(|arm| arm.call.is_some()),
                    "types": resolve.is_some_and(|arm| arm.types.is_some()),
                    "declared": resolve.is_some(),
                },
                "rehome": match rehome {
                    Some(arm) => json!({
                        "core": true,
                        "manifests": arm.manifests.is_some(),
                        "shim": arm.shim.is_some(),
                        "text_spellings": arm.text_spellings.is_some(),
                        "plan_check": arm.plan_check.is_some(),
                    }),
                    None => Value::Null,
                },
                "rename": rename,
                "checker": checker.map(|tier| tier.tool),
                "scip_indexer": indexer.map(|row| row.lang),
            })
        })
        .collect();

    for indexer in INDEXERS {
        if sources()
            .iter()
            .any(|source| source.name() == indexer_language(indexer.lang))
        {
            continue;
        }
        rows.push(json!({
            "language": indexer_language(indexer.lang),
            "source": false,
            "planes": [],
            "resolve": {"call": false, "types": false, "declared": false},
            "rehome": Value::Null,
            "rename": false,
            "checker": Value::Null,
            "scip_indexer": indexer.lang,
        }));
    }
    rows
}

pub fn jsonl() -> Vec<u8> {
    let mut output = Vec::new();
    for row in rows() {
        serde_json::to_writer(&mut output, &row).expect("capability row serializes");
        output.push(b'\n');
    }
    output
}

pub fn write_to(mut writer: impl Write) -> std::io::Result<()> {
    writer.write_all(&jsonl())
}
