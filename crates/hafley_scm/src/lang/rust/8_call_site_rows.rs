//! Rust call sites and const-initializer definitions from the caller's syn parse.

use std::collections::HashSet;
use std::ops::Range;

use syn::spanned::Spanned;

use super::call_metadata_rows::{cfg_test_predicate, item_attrs, path_string, span_range};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSiteRow {
    pub range: Range<u32>,
    pub callee: String,
    pub callee_path: Option<String>,
    pub cfg: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstInitRow {
    pub range: Range<u32>,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallSiteRows {
    pub sites: Vec<CallSiteRow>,
    pub const_inits: Vec<ConstInitRow>,
    pub test_only_calls: Vec<(String, String)>,
}

pub fn call_site_rows(
    parsed: &syn::File,
    line_starts: &[u32],
    def_ranges: &[Range<u32>],
) -> CallSiteRows {
    let mut collector = CallCollector {
        line_starts,
        sites: Vec::new(),
        under_cfg: None,
        defs: def_ranges,
        const_inits: Vec::new(),
        in_block: false,
    };
    syn::visit::visit_file(&mut collector, parsed);
    let shipped: HashSet<&str> = collector
        .sites
        .iter()
        .filter(|site| site.cfg.is_none())
        .map(|site| site.callee.as_str())
        .collect();
    let mut seen = HashSet::new();
    let test_only_calls = collector
        .sites
        .iter()
        .filter_map(|site| site.cfg.as_ref().map(|cfg| (&site.callee, cfg)))
        .filter(|(callee, _)| !shipped.contains(callee.as_str()))
        .filter(|(callee, _)| seen.insert(callee.as_str()))
        .map(|(callee, cfg)| (callee.clone(), cfg.clone()))
        .collect();
    CallSiteRows {
        sites: collector.sites,
        const_inits: collector.const_inits,
        test_only_calls,
    }
}

struct CallCollector<'a> {
    line_starts: &'a [u32],
    sites: Vec<CallSiteRow>,
    under_cfg: Option<String>,
    defs: &'a [Range<u32>],
    const_inits: Vec<ConstInitRow>,
    in_block: bool,
}

impl<'ast, 'a> syn::visit::Visit<'ast> for CallCollector<'a> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let outer = self.under_cfg.take();
        let own = cfg_test_predicate(item_attrs(item));
        self.under_cfg = outer.clone().or(own);
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
            let init = span_range(self.line_starts, expr.span());
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
                let start = span_range(self.line_starts, ident).start;
                let end = span_range(self.line_starts, expr.span()).end;
                self.const_inits.push(ConstInitRow {
                    range: start..end,
                    name,
                });
            }
        }
        self.under_cfg = outer;
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
                            range: span_range(self.line_starts, call.func.span()),
                            callee: segment.ident.to_string(),
                            callee_path: (path.path.segments.len() > 1)
                                .then(|| path_string(&path.path)),
                            cfg: self.under_cfg.clone(),
                        });
                    }
                }
                syn::visit::visit_expr(self, expr);
            }
            syn::Expr::MethodCall(call) => {
                self.sites.push(CallSiteRow {
                    range: span_range(self.line_starts, call.method.span()),
                    callee: call.method.to_string(),
                    callee_path: None,
                    cfg: self.under_cfg.clone(),
                });
                syn::visit::visit_expr(self, expr);
            }
            syn::Expr::Struct(struct_expr) => {
                if let Some(segment) = struct_expr
                    .path
                    .segments
                    .last()
                    .filter(|_| !is_variant_literal_path(&struct_expr.path))
                {
                    self.sites.push(CallSiteRow {
                        range: span_range(self.line_starts, struct_expr.path.span()),
                        callee: segment.ident.to_string(),
                        callee_path: (struct_expr.path.segments.len() > 1)
                            .then(|| path_string(&struct_expr.path)),
                        cfg: self.under_cfg.clone(),
                    });
                }
                syn::visit::visit_expr(self, expr);
            }
            _ => syn::visit::visit_expr(self, expr),
        }
    }
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
