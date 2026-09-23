//! Rust type-reference collection: the trailing path name of every named type
//! reference under a signature/type annotation, filtering primitive names.

use syn::{
    AngleBracketedGenericArguments, GenericArgument, Path, PathArguments, ReturnType, Type,
    TypeParamBound,
};
// The name-text helpers moved to `hafley_scm::lang::rust`; re-exported so the
// downstream imports here stay one path.
pub use hafley_scm::lang::rust::{path_name, primary_type};
use crate::types::TypeEdgeKind;

// ── type-reference collection (the arrow-type payload) ──────────────────────
//
// Port of v5 `type_refs`/`collect_type_refs`/`collect_bound_ref`/
// `collect_path_args`/`path_name`/`is_noise_type`. Collects the trailing path
// name of every named type reference under a signature annotation, filtering
// primitive names (`u32`, `str`, ...). One name per reference; a union slot
// stays one name (Rust has no inline union type syntax).

/// Every named type reference under `ty`, de-duplicated and sorted (port of v5
/// `type_refs`). Sorting makes the emitted sig order deterministic regardless of
/// syn traversal order.
pub(crate) fn type_refs(ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_refs(ty, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_type_refs(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Array(t) => collect_type_refs(&t.elem, out),
        Type::BareFn(t) => {
            for input in &t.inputs {
                collect_type_refs(&input.ty, out);
            }
            if let ReturnType::Type(_, ty) = &t.output {
                collect_type_refs(ty, out);
            }
        }
        Type::Group(t) => collect_type_refs(&t.elem, out),
        Type::ImplTrait(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Paren(t) => collect_type_refs(&t.elem, out),
        Type::Path(t) => {
            if let Some(qself) = &t.qself {
                collect_type_refs(&qself.ty, out);
            }
            if let Some(name) = path_name(&t.path) {
                out.push(name);
            }
            collect_path_args(&t.path, out);
        }
        Type::Ptr(t) => collect_type_refs(&t.elem, out),
        Type::Reference(t) => collect_type_refs(&t.elem, out),
        Type::Slice(t) => collect_type_refs(&t.elem, out),
        Type::TraitObject(t) => {
            for bound in &t.bounds {
                collect_bound_ref(bound, out);
            }
        }
        Type::Tuple(t) => {
            for elem in &t.elems {
                collect_type_refs(elem, out);
            }
        }
        _ => {}
    }
}

fn collect_bound_ref(bound: &TypeParamBound, out: &mut Vec<String>) {
    if let TypeParamBound::Trait(t) = bound {
        if let Some(name) = path_name(&t.path) {
            out.push(name);
        }
        collect_path_args(&t.path, out);
    }
}

pub(crate) fn collect_path_args(path: &Path, out: &mut Vec<String>) {
    for seg in &path.segments {
        match &seg.arguments {
            PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) => {
                for arg in args {
                    match arg {
                        GenericArgument::Type(t) => collect_type_refs(t, out),
                        GenericArgument::AssocType(t) => collect_type_refs(&t.ty, out),
                        GenericArgument::Constraint(c) => {
                            for bound in &c.bounds {
                                collect_bound_ref(bound, out);
                            }
                        }
                        _ => {}
                    }
                }
            }
            PathArguments::Parenthesized(p) => {
                for input in &p.inputs {
                    collect_type_refs(input, out);
                }
                if let ReturnType::Type(_, ty) = &p.output {
                    collect_type_refs(ty, out);
                }
            }
            PathArguments::None => {}
        }
    }
}

pub fn type_probe_key(name: &str, kind: TypeEdgeKind) -> (Option<&str>, &str) {
    // A Variant candidate's `to` is v5's synthetic `Enum::Variant` text, not a
    // path: text dsts stay text.
    match name.rsplit_once("::") {
        Some((qualifier, trailing)) if kind != TypeEdgeKind::Variant => (Some(qualifier), trailing),
        _ => (None, name),
    }
}
