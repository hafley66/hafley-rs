//! Declaration stores, scopes and lowering through the published RA APIs.
use ra_ap_hir::{InFile, ModuleDef, PathResolution, Semantics};
use ra_ap_hir_def::{
    ExpressionStoreOwnerId, FunctionId, GenericDefId, LoweringMode,
    expr_store::{
        Body, ExpressionStore, ExpressionStoreSourceMap, HygieneId, lower::ExprCollector,
        path::Path as HirPath, scope::ExprScopes,
    },
    hir::{Expr, ExprId, ExprOrPatId},
    resolver::{HasResolver, Resolver, TypeNs, ValueNs, resolver_for_scope},
};
use ra_ap_hir_ty::{
    LifetimeElisionKind, LifetimeLoweringMode, TyLoweringContext,
    next_solver::{DbInterner, Ty},
};
use ra_ap_ide::RootDatabase;
use ra_ap_syntax::{AstNode, ast};
use std::cell::OnceCell;

pub(super) struct DeclaredScope<'db, 'a> {
    pub db: &'db RootDatabase,
    pub sema: &'a Semantics<'db, RootDatabase>,
    pub owner: ExpressionStoreOwnerId,
    pub store: &'db ExpressionStore,
    pub map: &'db ExpressionStoreSourceMap,
}

impl<'db, 'a> DeclaredScope<'db, 'a> {
    pub fn for_node(
        db: &'db RootDatabase,
        sema: &'a Semantics<'db, RootDatabase>,
        node: &ra_ap_syntax::SyntaxNode,
    ) -> Option<Self> {
        let owner = sema
            .store_owner_for(InFile::new(sema.hir_file_for(node), node))?
            .try_into()
            .ok()?;
        let (store, map) = ExpressionStore::with_source_map(db, owner);
        Some(Self {
            db,
            sema,
            owner,
            store,
            map,
        })
    }

    pub fn expr_id(&self, expr: &ast::Expr) -> Option<ExprId> {
        match self
            .map
            .node_expr(InFile::new(self.sema.hir_file_for(expr.syntax()), expr))?
        {
            ExprOrPatId::ExprId(id) => Some(id),
            _ => None,
        }
    }

    pub fn resolver(&self, expr: &ast::Expr) -> Option<Resolver<'db>> {
        // A Names database has no Try lang items: RA may omit the operand
        // of `?` from its body store. Use the nearest stored expression's scope.
        let scopes = ExprScopes::of(self.db, self.owner);
        let lexical = expr
            .syntax()
            .ancestors()
            .filter_map(ast::Expr::cast)
            .filter_map(|expr| self.expr_id(&expr))
            .find_map(|id| scopes.scope_for(id));
        Some(resolver_for_scope(self.db, self.owner, lexical))
    }

    pub fn generic_def(&self) -> Option<GenericDefId> {
        self.owner.resolver(self.db).generic_def()
    }

    fn path(&self, expr: &ast::PathExpr) -> Option<(HirPath, HygieneId)> {
        if let Some(id) = self.expr_id(&expr.clone().into()) {
            if let Expr::Path(path) = &self.store[id] {
                return Some((path.clone(), self.store.expr_path_hygiene(id)));
            }
        }
        let resolver = self.resolver(&expr.clone().into())?;
        let mut collector = ExprCollector::new(
            self.db,
            resolver.module(),
            self.sema.hir_file_for(expr.syntax()),
            LoweringMode::Analysis,
        );
        let path =
            collector.lower_path(expr.path()?, &mut ExprCollector::impl_trait_error_allocator)?;
        // This fallback is for parsed physical source, whose syntax context is root.
        Some((path, HygieneId::ROOT))
    }

    pub fn value(&self, expr: &ast::PathExpr) -> Option<ValueNs> {
        let (path, hygiene) = self.path(expr)?;
        self.resolver(&expr.clone().into())?
            .resolve_path_in_value_ns_fully(self.db, &path, hygiene)
    }

    pub fn local_syntax(
        &self,
        binding: ra_ap_hir_def::hir::BindingId,
    ) -> Option<ra_ap_syntax::SyntaxNode> {
        let body_id = self.owner.as_def_with_body()?;
        let (body, map) = Body::with_source_map(self.db, body_id);
        if body.is_any_self_param(binding) {
            return Some(self.sema.to_node(map.self_param?).syntax().clone());
        }
        let patterns = map.patterns_for_binding(binding);
        let [pattern] = patterns else { return None };
        Some(
            self.sema
                .to_node(map.pat_syntax(*pattern).ok()?)
                .syntax()
                .clone(),
        )
    }

    pub fn lower_type(&self, ty: &ast::Type) -> Option<Ty<'db>> {
        let scope = Self::for_node(self.db, self.sema, ty.syntax())?;
        let resolver = scope.owner.resolver(self.db);
        let def = resolver.generic_def()?;
        let id = scope
            .map
            .node_type(InFile::new(self.sema.hir_file_for(ty.syntax()), ty))?;
        let interner = DbInterner::new_with(self.db, resolver.krate());
        let generics = OnceCell::new();
        let mut lowerer = TyLoweringContext::new(
            self.db,
            &resolver,
            scope.store,
            scope.owner,
            def,
            &generics,
            LifetimeElisionKind::Elided(interner.default_types().regions.erased),
            LifetimeLoweringMode::LateParam,
        );
        Some(lowerer.lower_ty(id))
    }

    pub fn function_id(&self, expr: &ast::PathExpr) -> Option<FunctionId> {
        if let Some(ValueNs::FunctionId(id)) = self.value(expr) {
            return Some(id);
        }
        // The module provider already owns associated path lookup. Its head is
        // resolved by RA's declaration scope; no caller-body inference is used.
        let path = expr.path()?;
        let scope = self.sema.scope(path.syntax())?;
        let ast_expr: ast::Expr = expr.clone().into();
        let (hir_path, _) = self.path(expr)?;
        let head = match self
            .resolver(&ast_expr)?
            .resolve_path_in_type_ns_fully(self.db, &hir_path.qualifier()?)?
        {
            TypeNs::SelfType(id) => PathResolution::SelfType(id.into()),
            TypeNs::GenericParam(id) => PathResolution::TypeParam(id.into()),
            TypeNs::AdtId(id) | TypeNs::AdtSelfType(id) => {
                PathResolution::Def(ModuleDef::Adt(id.into()))
            }
            TypeNs::EnumVariantId(id) => PathResolution::Def(ModuleDef::EnumVariant(id.into())),
            TypeNs::TypeAliasId(id) => PathResolution::Def(ModuleDef::TypeAlias(id.into())),
            TypeNs::BuiltinType(id) => PathResolution::Def(ModuleDef::BuiltinType(id.into())),
            TypeNs::TraitId(id) => PathResolution::Def(ModuleDef::Trait(id.into())),
            TypeNs::ModuleId(id) => PathResolution::Def(ModuleDef::Module(id.into())),
        };
        let name = path.segment()?.name_ref()?.text().to_string();
        let defs =
            super::names::associated_defs(self.db, scope.module(), head, Some(&scope), &name);
        match defs.as_slice() {
            [ra_ap_hir::ModuleDef::Function(function)] => (*function).try_into().ok(),
            _ => None,
        }
    }
}
