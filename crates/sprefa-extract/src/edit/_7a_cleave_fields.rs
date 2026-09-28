use std::collections::BTreeSet;

use sprefa_extract::{MoveCx, Span};

use super::cleave::rust_children;

pub(super) fn widen_private_fields(
    cx: &MoveCx,
    src: &str,
    dest: &str,
    source: &str,
    moving: &[Span],
    moving_text: &mut [String],
    checker: bool,
) -> Result<(), String> {
    if !src.ends_with(".rs") {
        return Ok(());
    }
    let parsed = hafley_scm::lang::rust::RustFastFile::extract(src, source.as_bytes())
        .ok_or_else(|| format!("parse Rust fields in {src}"))?;
    let mut private = Vec::new();
    for item in rust_children(parsed.tree().root_node()) {
        if item.kind() != "struct_item" {
            continue;
        }
        let Some(name_node) = item.child_by_field_name("name") else {
            continue;
        };
        let Some((index, span)) = moving.iter().enumerate().find(|(_, span)| {
            span.start as usize <= item.start_byte() && item.end_byte() <= span.end() as usize
        }) else {
            continue;
        };
        let Some(body) = item.child_by_field_name("body") else {
            continue;
        };
        for field in rust_children(body) {
            if field.kind() != "field_declaration"
                || rust_children(field)
                    .iter()
                    .any(|child| child.kind() == "visibility_modifier")
            {
                continue;
            }
            let Some(name) = field.child_by_field_name("name") else {
                continue;
            };
            private.push((
                source[name_node.byte_range()].to_string(),
                name_node.start_byte() as u32,
                name.start_byte() as u32,
                source[name.byte_range()].to_string(),
                index,
                name.start_byte() - span.start as usize,
            ));
        }
    }
    if private.is_empty() {
        return Ok(());
    }
    if !checker {
        let fields = private
            .iter()
            .map(|(struct_name, _, _, field_name, _, _)| format!("{struct_name}.{field_name}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "private_field_use_needs_checker: {fields}; rerun with --slow"
        ));
    }
    let probes: Vec<_> = private
        .iter()
        .map(|(_, struct_name_start, field_start, field_name, _, _)| {
            hafley_scm::read::lang::rust_checker::FieldProbe {
                struct_name_start: *struct_name_start,
                field_start: *field_start,
                field_name: field_name.clone(),
            }
        })
        .collect();
    let files: Vec<_> = cx
        .files()
        .iter()
        .filter(|rel| rel.ends_with(".rs"))
        .map(|rel| (rel.clone(), cx.abs(rel)))
        .collect();
    let reads = hafley_scm::read::lang::rust_checker::field_reads(
        cx.root(),
        &cx.abs(src),
        &files,
        &probes,
        std::time::Duration::from_secs(60),
    )
    .map_err(|error| format!("cleave cannot resolve moved Rust fields: {error}"))?;
    let needed: BTreeSet<u32> = reads
        .into_iter()
        .filter(|read| read.path != dest)
        .filter(|read| {
            read.path != src
                || !moving
                    .iter()
                    .any(|span| span.start <= read.access_start && read.access_start < span.end())
        })
        .map(|read| read.field_start)
        .collect();
    let mut inserts = Vec::new();
    for (_, _, field_start, _, index, offset) in private {
        if needed.contains(&field_start) {
            inserts.push((index, offset));
        }
    }
    inserts.sort_by(|a, b| b.cmp(a));
    for (index, offset) in inserts {
        moving_text[index].insert_str(offset, "pub(crate) ");
    }
    Ok(())
}
