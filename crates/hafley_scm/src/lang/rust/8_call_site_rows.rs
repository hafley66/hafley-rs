//! Rust call sites and const-initializer definitions from the caller's syn parse.

use std::ops::Range;

use syn::spanned::Spanned;

use super::call_metadata_rows::{path_string, span_range};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSiteRow {
    pub range: Range<u32>,
    pub callee: String,
    pub callee_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstInitRow {
    pub range: Range<u32>,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallSiteRows {
    pub sites: Vec<CallSiteRow>,
    pub expected_types: Vec<(Range<u32>, String)>,
    pub const_inits: Vec<ConstInitRow>,
}

pub fn call_site_rows(
    parsed: &syn::File,
    def_ranges: &[Range<u32>],
) -> CallSiteRows {
    let mut collector = CallCollector {
        sites: Vec::new(),
        expected_types: Vec::new(),
        defs: def_ranges,
        const_inits: Vec::new(),
        in_block: false,
    };
    syn::visit::visit_file(&mut collector, parsed);
    CallSiteRows {
        sites: collector.sites,
        expected_types: collector.expected_types,
        const_inits: collector.const_inits,
    }
}

struct CallCollector<'a> {
    sites: Vec<CallSiteRow>,
    expected_types: Vec<(Range<u32>, String)>,
    defs: &'a [Range<u32>],
    const_inits: Vec<ConstInitRow>,
    in_block: bool,
}

impl<'ast, 'a> syn::visit::Visit<'ast> for CallCollector<'a> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (syn::Pat::Type(pattern), Some(init)) = (&local.pat, &local.init) {
            if let (syn::Type::Path(ty), Some(call)) = (&*pattern.ty, default_call(&init.expr)) {
                if !ty.path.segments.is_empty() {
                    self.expected_types.push((
                        span_range(call.func.span()),
                        path_string(&ty.path),
                    ));
                }
            }
        }
        syn::visit::visit_local(self, local);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        let candidate = match item {
            syn::Item::Const(item) if !self.in_block => {
                Some((item.ident.span(), &item.expr, item.ident.to_string()))
            }
            syn::Item::Static(item) if !self.in_block => {
                Some((item.ident.span(), &item.expr, item.ident.to_string()))
            }
            _ => None,
        };
        let mark = self.sites.len();
        syn::visit::visit_item(self, item);
        if let Some((ident, expr, name)) = candidate {
            let init = span_range(expr.span());
            if self.sites[mark..]
                .iter()
                .filter(|site| init.start <= site.range.start && site.range.end <= init.end)
                .any(|site| {
                    !self
                        .defs
                        .iter()
                        .any(|range| range.start <= site.range.start && site.range.end <= range.end)
                })
            {
                let start = span_range(ident).start;
                let end = span_range(expr.span()).end;
                self.const_inits.push(ConstInitRow {
                    range: start..end,
                    name,
                });
            }
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let outer = self.in_block;
        self.in_block = true;
        syn::visit::visit_block(self, block);
        self.in_block = outer;
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            syn::Expr::Call(call) => {
                if let syn::Expr::Path(path) = peel_parens(&call.func) {
                    if let Some(segment) = path.path.segments.last() {
                        self.sites.push(CallSiteRow {
                            range: span_range(call.func.span()),
                            callee: segment.ident.to_string(),
                            callee_path: (path.path.segments.len() > 1)
                                .then(|| path_string(&path.path)),
                        });
                    }
                }
                syn::visit::visit_expr(self, expr);
            }
            syn::Expr::MethodCall(call) => {
                self.sites.push(CallSiteRow {
                    range: span_range(call.method.span()),
                    callee: call.method.to_string(),
                    callee_path: None,
                });
                syn::visit::visit_expr(self, expr);
            }
            syn::Expr::Struct(struct_expr) => {
                if let Some(rest) = &struct_expr.rest {
                    if let Some(call) = default_call(rest) {
                        self.expected_types.push((
                            span_range(call.func.span()),
                            path_string(&struct_expr.path),
                        ));
                    }
                }
                if let Some(segment) = struct_expr
                    .path
                    .segments
                    .last()
                    .filter(|_| !is_variant_literal_path(&struct_expr.path))
                {
                    self.sites.push(CallSiteRow {
                        range: span_range(struct_expr.path.span()),
                        callee: segment.ident.to_string(),
                        callee_path: (struct_expr.path.segments.len() > 1)
                            .then(|| path_string(&struct_expr.path)),
                    });
                }
                syn::visit::visit_expr(self, expr);
            }
            _ => syn::visit::visit_expr(self, expr),
        }
    }
}

fn default_call(expr: &syn::Expr) -> Option<&syn::ExprCall> {
    let syn::Expr::Call(call) = peel_parens(expr) else {
        return None;
    };
    let syn::Expr::Path(path) = peel_parens(&call.func) else {
        return None;
    };
    (path.path.segments.len() == 2
        && path.path.segments[0].ident == "Default"
        && path.path.segments[1].ident == "default")
        .then_some(call)
}

fn is_variant_literal_path(path: &syn::Path) -> bool {
    let count = path.segments.len();
    count >= 2
        && path.segments[count - 2]
            .ident
            .to_string()
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
}

fn peel_parens(expr: &syn::Expr) -> &syn::Expr {
    let mut current = expr;
    while let syn::Expr::Paren(paren) = current {
        current = &paren.expr;
    }
    current
}
