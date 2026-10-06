//! Written receivers through published declaration lowering and method lookup.
use super::declared_types::DeclaredScope;
use super::modules::{host_path, vfs_path};
use super::names::{Abstain, DefPlace, NamesHost};
use ra_ap_hir::{Function, Name, Semantics, attach_db};
use ra_ap_hir_def::{
    AdtId, FunctionId, VariantId,
    resolver::{HasResolver, ValueNs},
    signatures::VariantFields,
};
use ra_ap_hir_ty::{
    ParamEnvAndCrate, Span,
    db::HirDatabase,
    method_resolution::{CandidateId, MethodResolutionContext, Mode, PickKind},
    next_solver::{
        DbInterner, GenericArgs, Mutability, Ty,
        infer::{BoundRegionConversionTime, DbInternerInferExt, traits::ObligationCause},
        obligation_ctxt::ObligationCtxt,
    },
};
use ra_ap_ide::TryToNav;
use ra_ap_rustc_type_ir::{
    TypeVisitableExt, TypingMode,
    inherent::{GenericArgs as _, Ty as _},
};
use ra_ap_syntax::{AstNode, ast};
use std::path::{Path, PathBuf};

pub fn resolve_written_method(
    host: &NamesHost,
    file: &Path,
    offset: u32,
    method: &str,
) -> Result<Vec<DefPlace>, Abstain> {
    if super::names::module_places(host, file)?.len() != 1 {
        return Err(Abstain::NeedsTypes);
    }
    let mut workspace = host.workspace.lock().unwrap();
    let key = (host_path(file), offset, method.to_string());
    if let Some(answer) = workspace.names.written.get(&key) {
        return answer.clone();
    }
    let (id, _) = workspace
        .vfs
        .file_id(&vfs_path(&host_path(file)))
        .ok_or(Abstain::NeedsTypes)?;
    let db = workspace.host.raw_database();
    let answer = attach_db(db, || {
        let sema = Semantics::new(db);
        let parsed = sema.parse_guess_edition(ra_ap_ide::FileId::from_raw(id.index()));
        let call = parsed
            .syntax()
            .descendants()
            .filter_map(ast::MethodCallExpr::cast)
            .find(|call| {
                call.name_ref()
                    .is_some_and(|name| u32::from(name.syntax().text_range().start()) == offset)
            })
            .ok_or(Abstain::NeedsTypes)?;
        let scope = DeclaredScope::for_node(db, &sema, call.syntax()).ok_or(Abstain::NeedsTypes)?;
        let ty = written_type(&scope, call.receiver().ok_or(Abstain::NeedsTypes)?, 0)
            .ok_or(Abstain::NeedsTypes)?;
        let function =
            candidate(&scope, &call.clone().into(), ty, method).ok_or(Abstain::NeedsTypes)?;
        let nav = function
            .try_to_nav(&sema)
            .map(|nav| nav.call_site)
            .ok_or(Abstain::NeedsTypes)?;
        let path = workspace
            .vfs
            .file_path(ra_ap_vfs::FileId::from_raw(nav.file_id.index()));
        let path = path
            .as_path()
            .map(|path| PathBuf::from(path.to_string()))
            .ok_or(Abstain::NeedsTypes)?;
        let root = workspace.vfs.file_path(ra_ap_vfs::FileId::from_raw(
            function.module(db).krate(db).root_file(db).index(),
        ));
        if !root.as_path().is_some_and(|path| {
            host.crate_roots
                .contains_key(&PathBuf::from(path.to_string()))
        }) {
            return Err(Abstain::NeedsTypes);
        }
        let range = nav.focus_range.unwrap_or(nav.full_range);
        Ok(vec![DefPlace {
            file: path,
            name: nav.name.to_string(),
            start: range.start().into(),
            end: range.end().into(),
            module: false,
        }])
    });
    workspace.names.written.insert(key, answer.clone());
    answer
}

fn candidate<'db>(
    scope: &DeclaredScope<'db, '_>,
    expr: &ast::Expr,
    ty: Ty<'db>,
    method: &str,
) -> Option<Function> {
    let resolver = scope.resolver(expr)?;
    let db = scope.db;
    let interner = DbInterner::new_with(db, resolver.krate());
    let mode = match resolver.expression_store_owner() {
        Some(owner) => TypingMode::analysis_in_body(interner, owner.into()),
        None => TypingMode::non_body_analysis(),
    };
    let infcx = interner.infer_ctxt().build(mode);
    let traits = resolver.traits_in_scope(db);
    let env = db.trait_environment(scope.generic_def()?);
    let ctx = MethodResolutionContext {
        infcx: &infcx,
        resolver: &resolver,
        param_env: env,
        traits_in_scope: &traits,
        edition: resolver.def_map().edition(),
        features: resolver.top_level_def_map().features(),
        call_span: Span::Dummy,
        receiver_span: Span::Dummy,
    };
    let canonical = ra_ap_hir_ty::replace_errors_with_variables(interner, &ty);
    let (receiver, _) = infcx.instantiate_canonical(Span::Dummy, &canonical);
    let pick = ctx
        .probe_for_name(Mode::MethodCall, Name::new_root(method), receiver)
        .ok()?;
    let CandidateId::FunctionId(id) = pick.item else {
        return None;
    };
    if let PickKind::TraitPick(trait_id) = pick.kind {
        let trait_: ra_ap_hir::Trait = trait_id.into();
        if trait_.type_or_const_param_count(db, false) != 0
            || !ra_ap_hir::GenericDef::Trait(trait_)
                .lifetime_params(db)
                .is_empty()
        {
            return None;
        }
        let receiver = ra_ap_rustc_type_ir::fold_regions(interner, pick.self_ty, |_, _| {
            interner.default_types().regions.erased
        });
        let fresh = infcx.fresh_args_for_item(Span::Dummy, id.into());
        let signature = db
            .callable_item_signature(id.into())
            .instantiate(interner, fresh.as_slice())
            .skip_norm_wip();
        let signature = infcx.instantiate_binder_with_fresh_vars(
            Span::Dummy,
            BoundRegionConversionTime::FnCall,
            signature,
        );
        let relation = infcx
            .at(&ObligationCause::dummy(), env)
            .eq(signature.inputs()[0], receiver)
            .ok()?;
        let mut obligations = ObligationCtxt::new(&infcx);
        obligations.register_infer_ok_obligations(relation);
        if !obligations
            .evaluate_obligations_error_on_ambiguity()
            .is_empty()
        {
            return None;
        }
        let self_ty = infcx.resolve_vars_if_possible(fresh.type_at(0));
        if self_ty.has_non_region_infer() {
            return None;
        }
        let self_ty = ra_ap_rustc_type_ir::fold_regions(interner, self_ty, |_, _| {
            interner.default_types().regions.erased
        });
        let args = GenericArgs::new_from_slice(&[self_ty.into()]);
        let (selected, _) = db.lookup_impl_method(
            ParamEnvAndCrate {
                param_env: env,
                krate: resolver.krate(),
            },
            id,
            args,
        );
        // Builtin derives are outside this written workspace-method contract.
        let target = selected.left()?;
        let function: Function = target.into();
        if target == id && !function.has_body(db) {
            return None;
        }
        Some(function)
    } else {
        Some(id.into())
    }
}

fn written_type<'db>(
    scope: &DeclaredScope<'db, '_>,
    expr: ast::Expr,
    depth: usize,
) -> Option<Ty<'db>> {
    if depth > 16 {
        return None;
    }
    let db = scope.db;
    let interner = DbInterner::new_with(db, scope.owner.resolver(db).krate());
    let ty = match expr {
        ast::Expr::PathExpr(expr) => match scope.value(&expr)? {
            ValueNs::LocalBinding(binding) => {
                let node = scope.local_syntax(binding)?;
                if let Some(param) = node.ancestors().find_map(ast::Param::cast) {
                    if param.pat()?.syntax() != &node {
                        return None;
                    }
                    scope.lower_type(&param.ty()?)?
                } else if let Some(param) = ast::SelfParam::cast(node.clone()) {
                    let ty = if let Some(ty) = param.ty() {
                        scope.lower_type(&ty)?
                    } else {
                        let resolver = scope.owner.resolver(db);
                        db.impl_self_ty(resolver.impl_def()?)
                            .instantiate_identity()
                            .skip_norm_wip()
                    };
                    if param.ty().is_none() && param.amp_token().is_some() {
                        Ty::new_ref(
                            interner,
                            interner.default_types().regions.erased,
                            ty,
                            if param.mut_token().is_some() {
                                Mutability::Mut
                            } else {
                                Mutability::Not
                            },
                        )
                    } else {
                        ty
                    }
                } else {
                    let binding = node.ancestors().find_map(ast::LetStmt::cast)?;
                    if binding.pat()?.syntax() != &node {
                        return None;
                    }
                    if let Some(ty) = binding.ty() {
                        scope.lower_type(&ty)?
                    } else {
                        written_type(scope, binding.initializer()?, depth + 1)?
                    }
                }
            }
            ValueNs::StructId(id) => db
                .ty(AdtId::StructId(id).into())
                .instantiate_identity()
                .skip_norm_wip(),
            _ => return None,
        },
        ast::Expr::ParenExpr(expr) => written_type(scope, expr.expr()?, depth + 1)?,
        ast::Expr::RefExpr(expr) => Ty::new_ref(
            interner,
            interner.default_types().regions.erased,
            written_type(scope, expr.expr()?, depth + 1)?,
            if expr.mut_token().is_some() {
                Mutability::Mut
            } else {
                Mutability::Not
            },
        ),
        ast::Expr::FieldExpr(expr) => {
            let (adt, args) = written_type(scope, expr.expr()?, depth + 1)?
                .strip_references()
                .as_adt()?;
            let variant = match adt {
                AdtId::StructId(id) => VariantId::StructId(id),
                AdtId::UnionId(id) => VariantId::UnionId(id),
                _ => return None,
            };
            let fields = VariantFields::of(db, variant);
            let name = expr.name_ref()?.text().to_string();
            let (field, _) = fields
                .fields()
                .iter()
                .find(|(_, field)| field.name.as_str() == name)?;
            db.field_types(variant)[field]
                .ty()
                .instantiate(interner, args.as_slice())
                .skip_norm_wip()
        }
        ast::Expr::CallExpr(expr) => {
            let ast::Expr::PathExpr(path) = expr.expr()? else {
                return None;
            };
            if let Some(id) = scope.function_id(&path) {
                signature_return(scope, id)?
            } else {
                match scope.value(&path)? {
                    ValueNs::StructId(id) => db
                        .ty(AdtId::StructId(id).into())
                        .instantiate_identity()
                        .skip_norm_wip(),
                    ValueNs::EnumVariantId(id) => {
                        let variant: ra_ap_hir::EnumVariant = id.into();
                        let adt: AdtId = ra_ap_hir::Adt::Enum(variant.parent_enum(db)).into();
                        db.ty(adt.into()).instantiate_identity().skip_norm_wip()
                    }
                    _ => return None,
                }
            }
        }
        ast::Expr::MethodCallExpr(expr) => {
            let ty = written_type(scope, expr.receiver()?, depth + 1)?;
            let function = candidate(scope, &expr.clone().into(), ty, expr.name_ref()?.text())?;
            signature_return(scope, function.try_into().ok()?)?
        }
        _ => return None,
    };
    (!ty.references_non_lt_error()).then_some(ty)
}

fn signature_return<'db>(scope: &DeclaredScope<'db, '_>, function: FunctionId) -> Option<Ty<'db>> {
    let interner = DbInterner::new_no_crate(scope.db);
    let signature = scope
        .db
        .callable_item_signature(function.into())
        .instantiate_identity()
        .skip_norm_wip();
    let ty = interner
        .instantiate_bound_regions_with_erased(signature)
        .output();
    (!ty.has_param() && !ty.references_non_lt_error()).then_some(ty)
}
