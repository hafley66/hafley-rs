use sprefa_extract::{dispatch, flatten, FamilyMask, FamilyTag, FlatFact};
use std::collections::BTreeSet;

pub fn line_of(bytes: &[u8], byte_off: u32) -> u32 {
    bytes[..byte_off as usize]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

pub fn facet_of(line: &str) -> &str {
    line.split('\t').next().unwrap_or("")
}

pub fn v6_ported(path: &str, bytes: &[u8]) -> BTreeSet<String> {
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches the fixture");
    let facts = flatten(&out);
    let entity: std::collections::BTreeMap<(u32, u32), (String, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                span,
                kind,
                name: Some(name),
                ..
            } => Some(((span.start, span.end), (kind.clone(), name.clone()))),
            _ => None,
        })
        .collect();
    let captures = crate::v5_parity::capture_parity::capture_edges(&facts);
    let df_index: std::collections::HashMap<u32, u32> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Df,
                span,
                ..
            } => Some(span.start),
            _ => None,
        })
        .enumerate()
        .map(|(ix, start)| (start, ix as u32))
        .collect();
    let interface_names: std::collections::BTreeSet<&str> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                kind,
                name: Some(name),
                ..
            } if kind == "interface" => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let interface_spec_spans: std::collections::BTreeSet<(u32, u32)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::MethodOwnerOut {
                owner,
                self_type: Some(self_type),
                ..
            } if interface_names.contains(self_type.as_str()) => Some((owner.start, owner.end)),
            _ => None,
        })
        .collect();
    let mut set = BTreeSet::new();
    for fact in facts {
        match fact {
            FlatFact::Node {
                family,
                span,
                kind,
                name,
                ..
            } => match family {
                FamilyTag::Type => {
                    set.insert(format!(
                        "type_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        line_of(bytes, span.start)
                    ));
                }
                FamilyTag::Call => {
                    if !interface_spec_spans.contains(&(span.start, span.end)) {
                        let name = name.filter(|n| !n.starts_with("<lambda"));
                        set.insert(format!(
                            "call_def\t{kind}\t{}\t{}",
                            name.as_deref().unwrap_or(""),
                            line_of(bytes, span.start)
                        ));
                    }
                }
                FamilyTag::Df => {
                    set.insert(format!(
                        "df_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        span.start
                    ));
                }
                FamilyTag::Cst
                | FamilyTag::Module
                | FamilyTag::Flow
                | FamilyTag::Cfg
                | FamilyTag::Data => {}
            },
            FlatFact::Edge {
                family, from, to, ..
            } => match family {
                FamilyTag::Df
                    if !captures.contains(&((from.start, from.end), (to.start, to.end))) =>
                {
                    set.insert(format!("df_edge\t{}\t{}", from.start, to.start));
                }
                FamilyTag::Cst => {}
                _ => {}
            },
            FlatFact::DfParam { .. } | FlatFact::DfArg { .. } => {}
            FlatFact::DfField {
                owner, name, value, ..
            } => {
                set.insert(format!(
                    "df_fields\t{}\t{name}\t{}",
                    df_index[&owner.start], df_index[&value.start]
                ));
            }
            FlatFact::DfLit {
                node, kind, text, ..
            } => {
                set.insert(format!(
                    "df_lits\t{}\t{kind}\t{text}",
                    df_index[&node.start]
                ));
            }
            FlatFact::Sig {
                owner,
                slot,
                pos,
                ty,
                ..
            } => {
                set.insert(format!(
                    "type_sig\t{}\t{slot}\t{pos}\t{ty}",
                    line_of(bytes, owner.start)
                ));
            }
            FlatFact::Site { span, callee, .. } => {
                set.insert(format!(
                    "call_site\t{callee}\t{}",
                    line_of(bytes, span.start)
                ));
            }
            FlatFact::Const {
                owner,
                field,
                text,
                kind,
                ..
            } => {
                set.insert(format!(
                    "const_value\t{}\t{}\t{kind}\t{text}",
                    line_of(bytes, owner.start),
                    field.as_deref().unwrap_or(""),
                ));
            }
            FlatFact::Doc { owner, parent, .. } => {
                if let Some((kind, name)) = entity.get(&(owner.start, owner.end)) {
                    let qualified = match parent {
                        Some(owner_type) => format!("{owner_type}.{name}"),
                        None => name.clone(),
                    };
                    set.insert(format!(
                        "doc\t{path}::{kind}::{qualified}\t{}",
                        line_of(bytes, owner.start)
                    ));
                }
            }
            FlatFact::DocTagOut { .. } => {}
            FlatFact::ProjectEdge { .. } => {}
            FlatFact::FlowEdgeOut { .. } => {}
            FlatFact::Specifier { .. } => {}
            FlatFact::Reference { .. } => {}
            FlatFact::ResolvedEdge { .. } => {}
            FlatFact::ResolvedTypeEdge { .. } => {}
            FlatFact::ScipMetadataRow { .. } => {}
            FlatFact::ScipDocumentRow { .. } => {}
            FlatFact::ScipOccurrenceRow { .. } => {}
            FlatFact::ScipOccurrenceDocRow { .. } => {}
            FlatFact::ScipDiagnosticRow { .. } => {}
            FlatFact::ScipSymbolRow { .. } => {}
            FlatFact::ScipRelationshipRow { .. } => {}
            FlatFact::ScipDocumentationRow { .. } => {}
            FlatFact::ScipSignatureRow { .. } => {}
            FlatFact::ScipSignatureOccurrenceRow { .. } => {}
            FlatFact::FileRow { .. } => {}
            FlatFact::FileEdgeRow { .. } => {}
            FlatFact::ScipDefRow { .. } => {}
            FlatFact::ScipNameRow { .. } => {}
            FlatFact::ScipRefRow { .. } => {}
            FlatFact::ScipEdgeRow { .. } => {}
            FlatFact::ScipFnEdgeRow { .. } => {}
            FlatFact::ScipCalleeTypeRow { .. } => {}
            FlatFact::ScipLocalRow { .. } => {}
            FlatFact::ScipImplRow { .. } => {}
            FlatFact::ScipIndexRow { .. } => {}
            FlatFact::ScipSkipRow { .. } => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
    set
}
