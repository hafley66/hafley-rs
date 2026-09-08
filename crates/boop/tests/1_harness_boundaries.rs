//! Parse production Rust instead of treating comments, tests and schema values
//! as behavioral dispatch. The same visitor writes the audit inventory.

use proc_macro2::{TokenStream, TokenTree};
use std::path::{Path, PathBuf};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

fn marker(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Group(group) => marker(group.stream()),
        TokenTree::Ident(id) => matches!(
            id.to_string().as_str(),
            "Claude" | "Codex" | "Kimi" | "Opencode"
        ),
        TokenTree::Literal(value) => matches!(
            value.to_string().as_str(),
            "\"claude\"" | "\"codex\"" | "\"kimi\"" | "\"opencode\""
        ),
        _ => false,
    })
}

fn test_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string() == "test"))
    })
}

#[derive(Default)]
struct Inventory {
    path: String,
    symbols: Vec<String>,
    test: bool,
    rows: Vec<serde_json::Value>,
}

impl Inventory {
    fn macro_markers(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Group(group) => self.macro_markers(group.stream()),
                TokenTree::Ident(id)
                    if matches!(
                        id.to_string().as_str(),
                        "HarnessId" | "Claude" | "Codex" | "Kimi" | "Opencode"
                    ) =>
                {
                    self.record(id.span(), "macro-enum-reference", id.to_string());
                }
                TokenTree::Literal(value) => {
                    if let Ok(literal) = syn::parse_str::<syn::LitStr>(&value.to_string()) {
                        if matches!(
                            literal.value().as_str(),
                            "claude" | "codex" | "kimi" | "opencode"
                        ) {
                            self.record(value.span(), "macro-name-literal", literal.value());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn record(&mut self, span: proc_macro2::Span, role: &str, spelling: String) {
        self.rows.push(
            serde_json::json!({"file":self.path,"line":span.start().line,
            "symbol":self.symbols.join("::"),"role":role,"test":self.test,"spelling":spelling}),
        );
    }

    fn scoped(&mut self, name: String, test: bool, run: impl FnOnce(&mut Self)) {
        self.symbols.push(name);
        let before = self.test;
        self.test |= test;
        run(self);
        self.test = before;
        self.symbols.pop();
    }
}

impl<'ast> Visit<'ast> for Inventory {
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let name = match item.self_ty.as_ref() {
            syn::Type::Path(path) => path
                .path
                .segments
                .iter()
                .map(|p| p.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
            _ => "impl".into(),
        };
        self.scoped(name, test_attrs(&item.attrs), |this| {
            visit::visit_item_impl(this, item)
        });
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.scoped(item.ident.to_string(), test_attrs(&item.attrs), |this| {
            visit::visit_item_trait(this, item)
        });
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.scoped(
            item.sig.ident.to_string(),
            test_attrs(&item.attrs),
            |this| visit::visit_trait_item_fn(this, item),
        );
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.scoped(item.ident.to_string(), test_attrs(&item.attrs), |this| {
            visit::visit_item_struct(this, item)
        });
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.scoped(item.ident.to_string(), test_attrs(&item.attrs), |this| {
            visit::visit_item_mod(this, item)
        });
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.scoped(
            item.sig.ident.to_string(),
            test_attrs(&item.attrs),
            |this| visit::visit_item_fn(this, item),
        );
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.scoped(
            item.sig.ident.to_string(),
            test_attrs(&item.attrs),
            |this| visit::visit_impl_item_fn(this, item),
        );
    }
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.scoped(item.ident.to_string(), test_attrs(&item.attrs), |this| {
            visit::visit_item_const(this, item)
        });
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.scoped(item.ident.to_string(), test_attrs(&item.attrs), |this| {
            visit::visit_item_static(this, item)
        });
    }
    fn visit_expr_match(&mut self, item: &'ast syn::ExprMatch) {
        // Patterns carry the dispatch key; a string in a result arm alone is
        // static output data and remains in the literal inventory.
        if item.arms.iter().any(|arm| marker(quote_pat(&arm.pat))) {
            self.record(item.span(), "dispatch:match", "match".into());
        }
        visit::visit_expr_match(self, item);
    }
    fn visit_expr_binary(&mut self, item: &'ast syn::ExprBinary) {
        if matches!(item.op, syn::BinOp::Eq(_) | syn::BinOp::Ne(_))
            && (expr_marker(&item.left) || expr_marker(&item.right))
        {
            self.record(item.span(), "dispatch:comparison", "comparison".into());
        }
        visit::visit_expr_binary(self, item);
    }
    fn visit_expr_if(&mut self, item: &'ast syn::ExprIf) {
        if let syn::Expr::Let(binding) = item.cond.as_ref() {
            if marker(quote_pat(&binding.pat)) {
                self.record(item.span(), "dispatch:if-let", "if let".into());
            }
        }
        visit::visit_expr_if(self, item);
    }
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        if matches!(
            item.method.to_string().as_str(),
            "eq" | "ne" | "contains" | "starts_with" | "ends_with" | "strip_prefix"
        ) && item.args.iter().any(expr_marker)
        {
            self.record(
                item.span(),
                "dispatch:string-method",
                item.method.to_string(),
            );
        }
        visit::visit_expr_method_call(self, item);
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if item.path.is_ident("matches") && marker(item.tokens.clone()) {
            self.record(
                item.span(),
                "dispatch:matches-macro",
                item.tokens.to_string(),
            );
        }
        self.macro_markers(item.tokens.clone());
        visit::visit_macro(self, item);
    }
    fn visit_path(&mut self, item: &'ast syn::Path) {
        if item.segments.iter().any(|part| part.ident == "HarnessId") {
            self.record(
                item.span(),
                "enum-reference",
                item.segments
                    .iter()
                    .map(|p| p.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        } else if item.segments.last().is_some_and(|part| {
            matches!(
                part.ident.to_string().as_str(),
                "Claude" | "Codex" | "Kimi" | "Opencode"
            )
        }) {
            self.record(
                item.span(),
                "variant-reference",
                item.segments
                    .iter()
                    .map(|p| p.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        visit::visit_path(self, item);
    }
    fn visit_lit_str(&mut self, item: &'ast syn::LitStr) {
        if matches!(
            item.value().as_str(),
            "claude" | "codex" | "kimi" | "opencode"
        ) {
            self.record(item.span(), "name-literal", item.value());
        }
    }
}

// Reuse syn's tree traversal to find marker paths/literals without a second
// parser or a source-text approximation of a Rust expression.
fn expr_marker(expr: &syn::Expr) -> bool {
    let mut finder = Inventory::default();
    finder.visit_expr(expr);
    finder.rows.iter().any(|row| {
        row["role"] == "name-literal"
            || ((row["role"] == "enum-reference" || row["role"] == "variant-reference")
                && matches!(
                    row["spelling"].as_str().unwrap_or("").rsplit("::").next(),
                    Some("Claude" | "Codex" | "Kimi" | "Opencode")
                ))
    })
}

fn quote_pat(pat: &syn::Pat) -> TokenStream {
    // syn patterns contain expressions for literal/path patterns. A small
    // visitor covers nested Some(...), alternatives and bindings alike.
    let mut finder = Inventory::default();
    finder.visit_pat(pat);
    finder
        .rows
        .iter()
        .filter_map(|row| {
            let value = row["spelling"].as_str()?;
            match row["role"].as_str()? {
                "name-literal" => Some(format!("{value:?}")),
                "enum-reference" | "variant-reference" => Some(value.to_owned()),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .parse()
        .unwrap()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("target" | "node_modules" | ".git")
            ) {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn allowed(row: &serde_json::Value) -> bool {
    let file = row["file"].as_str().unwrap();
    row["test"] == true
        || file.contains("/tests/")
        || file.starts_with("crates/boop-harness/")
        || file == "crates/boop-acp/src/channel/claude.rs"
        || file == "crates/boop-acp/src/channel/1_acpx.rs"
        || (file == "crates/boop-store/src/harness_id.rs" && row["symbol"] == "HarnessId::as_str")
}

#[test]
fn behavioral_harness_dispatch_stays_in_adapters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    files.sort();
    let mut inventory = Inventory::default();
    for path in files {
        inventory.path = path.strip_prefix(root).unwrap().display().to_string();
        inventory.test =
            inventory.path.contains("/tests/") || inventory.path.ends_with("_tests.rs");
        let source = std::fs::read_to_string(&path).unwrap();
        let ast =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        inventory.visit_file(&ast);
    }
    if let Some(path) = std::env::var_os("BOOP_HARNESS_INVENTORY") {
        std::fs::write(path, serde_json::to_vec_pretty(&inventory.rows).unwrap()).unwrap();
    }
    let forbidden: Vec<_> = inventory
        .rows
        .iter()
        .filter(|row| row["role"].as_str().unwrap().starts_with("dispatch:") && !allowed(row))
        .collect();
    assert!(
        forbidden.is_empty(),
        "behavioral harness branches outside adapters:\n{}",
        serde_json::to_string_pretty(&forbidden).unwrap()
    );
}

#[test]
fn guard_detects_enum_string_and_macro_dispatch_and_separates_tests() {
    let source = r#"
        const DEFAULT: HarnessId = HarnessId::Codex;
        fn a(id: HarnessId, name: &str) {
            match id { HarnessId::Codex => (), _ => () }
            if name == "codex" {}
            if let Some(HarnessId::Claude) = Some(id) {}
            if matches!(id, HarnessId::Kimi) {}
            if name.starts_with("opencode") {}
        }
        #[cfg(test)] mod tests { fn b(id: HarnessId) { if id == HarnessId::Codex {} } }
    "#;
    let mut inventory = Inventory::default();
    inventory.visit_file(&syn::parse_file(source).unwrap());
    let roles: Vec<_> = inventory
        .rows
        .iter()
        .filter(|row| row["role"].as_str().unwrap().starts_with("dispatch:"))
        .map(|row| {
            (
                row["role"].as_str().unwrap(),
                row["test"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        roles,
        [
            ("dispatch:match", false),
            ("dispatch:comparison", false),
            ("dispatch:if-let", false),
            ("dispatch:matches-macro", false),
            ("dispatch:string-method", false),
            ("dispatch:comparison", true)
        ]
    );
}
