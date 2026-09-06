//! `egui_rsx! { ... }` — parse dioxus-rsx grammar, emit plain egui immediate-mode calls.
//!
//! Constraint: this crate only *reads* the dioxus-rsx AST. It never calls the AST's own
//! `ToTokens` impls — those emit dioxus_core code (Template/VNode/...). We fold the parsed
//! tree into egui calls ourselves per the mapping table in plans/rsx-egui-mvp.md.
//!
//! Expansion runs against an ambient `ui: &mut egui::Ui` at the call site. Event handlers and
//! `{expr}` splices close over whatever the call site has in scope (`state`, `out`, ...).
//!
//! Lab 2: an optional `stylesheet: "file.css",` header resolves `class:`/element selectors
//! against a real CSS file AT EXPANSION TIME. The macro sees the whole static element tree, so
//! selector matching + the cascade run in the proc macro (via `simplecss`); computed values are
//! baked into the egui calls as literals. No runtime cascade, no shadow tree. `:hover`/`:active`
//! map onto egui per-state widget visuals inside a scoped `ui.visuals_mut()` override.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dioxus_rsx::{
    AttributeName, AttributeValue, BodyNode, CallBody, Component, Element, ElementName, ForLoop,
    HotLiteral, IfChain, PartialClosure, TextNode,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Token};

// --- macro entry -----------------------------------------------------------

// Optional CSS header peeled off the token stream BEFORE handing the rest to dioxus_rsx's
// CallBody. CallBody::parse would choke on `stylesheet: "..",` (top-level rsx expects nodes), so
// we consume it here first. See README "stylesheet header parsing".
struct CssHeader {
    path: String,
    span: proc_macro2::Span,
}

struct RsxInput {
    css: Option<CssHeader>,
    body: CallBody,
}

impl Parse for RsxInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Header form is exactly `stylesheet : "literal" ,`. A top-level rsx node is `tag { .. }`
        // (ident followed by a brace), so `ident :` unambiguously flags the header.
        let mut css = None;
        if input.peek(Ident) && input.peek2(Token![:]) {
            let ahead = input.fork();
            let id: Ident = ahead.parse()?;
            if id == "stylesheet" {
                let id: Ident = input.parse()?;
                let _colon: Token![:] = input.parse()?;
                let lit: LitStr = input.parse()?;
                let _comma: Token![,] = input.parse()?;
                css = Some(CssHeader {
                    path: lit.value(),
                    span: id.span(),
                });
            }
        }
        let body: CallBody = input.parse()?;
        Ok(RsxInput { css, body })
    }
}

#[proc_macro]
pub fn egui_rsx(input: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<RsxInput>(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    let ctx = match build_ctx(&parsed) {
        Ok(ctx) => ctx,
        Err(err) => return err.into(),
    };

    let expanded = emit_roots(&parsed.body.body.roots, &ctx);

    // Debug dump: `EGUI_RSX_DUMP=<path> cargo build` writes the pretty-printed expansion of each
    // STYLED view so the baked style literals can be checked into expanded.rs. Off by default. Both
    // styled views (Lab 2 `view_styled`, Lab 3 `view_xp`) land in the one file; the first-declared
    // (styles.css) truncates, the rest append, so a full rebuild yields a stable, ordered dump.
    if ctx.styles.is_some() {
        if let Some(path) = std::env::var_os("EGUI_RSX_DUMP") {
            if let Some(header) = &parsed.css {
                let stem = std::path::Path::new(&header.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                dump_expansion(&expanded, &path, &stem);
            }
        }
    }

    expanded.into()
}

// --- #[component] ----------------------------------------------------------
//
// Lab 6 slice 2: kill the per-component Props boilerplate. The user writes a single fn with
// positional args (ui first, children last, props in the middle); the macro generates the Props
// struct + Default impl + a wrapper that the slice-1 invocation lowering calls into.
//
//   #[component]
//   fn xp_button(ui: &mut egui::Ui, label: &'static str, children: impl FnOnce(&mut egui::Ui)) {
//       egui_rsx! { ... }
//   }
//
// expands to:
//   fn xp_button_def(ui, label, children) { ... }            // user's body, renamed
//   #[derive(Default)]
//   struct XpButtonProps { pub label: &'static str }
//   fn xp_button(ui, props: XpButtonProps, children) { xp_button_def(ui, props.label, children) }
//
// Then slice-1 invocation `<XpButton label="OK" />` lowers to `xp_button(ui, XpButtonProps { label:
// "OK", ..Default::default() }, children_closure)` which calls the wrapper which calls the user's
// body. Convention is rigid: first arg = `ui`, last arg = `children` (typed `impl FnOnce(&mut
// egui::Ui)`), middle args = Props fields. The macro uses arg POSITION, not names, so the user can
// name `ui`/`children` whatever they want.

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_fn = match syn::parse::<syn::ItemFn>(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };
    match expand_component(item_fn) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// Pure: take the parsed ItemFn, emit the wrapper + struct + renamed body. Errors are syn::Error so
// they render as compile_errors with spans (pointing at the offending arg, the missing children,
// etc.). The body's tokens are spliced verbatim into the renamed fn -- no AST rewriting.
fn expand_component(item: syn::ItemFn) -> Result<TokenStream2, syn::Error> {
    use syn::{FnArg, ItemFn, Pat, Signature};

    let vis = &item.vis;
    let sig = &item.sig;
    let fn_name = sig.ident.clone();
    let fn_name_str = fn_name.to_string();
    let pascal = to_pascal_case(&fn_name_str);
    let props_ident = Ident::new(&format!("{pascal}Props"), fn_name.span());
    let inner_ident = Ident::new(&format!("{fn_name_str}_def"), fn_name.span());

    // Validate + slice the arg list. Required shape:
    //   (ui_arg, prop_args.., children_arg)
    // ui_arg + children_arg are runtime plumbing; prop_args become Props fields.
    let inputs = &sig.inputs;
    if inputs.len() < 2 {
        return Err(syn::Error::new(
            sig.ident.span(),
            "expected at least 2 args: (ui, ..., children); found fewer",
        ));
    }
    for arg in inputs.iter() {
        if let FnArg::Receiver(_) = arg {
            return Err(syn::Error::new_spanned(
                arg,
                "components can't be methods (no self)",
            ));
        }
    }
    let mut iter = inputs.iter();
    let first = iter.next().expect(">= 2 args (checked)");
    let last = iter.last().expect(">= 2 args (checked)");
    let middle: Vec<&FnArg> = inputs.iter().take(inputs.len() - 1).skip(1).collect();

    // Build Props struct field declarations + the list of field idents used in the wrapper call.
    let mut prop_fields = Vec::new();
    let mut prop_field_idents = Vec::new();
    for arg in &middle {
        let FnArg::Typed(pt) = arg else {
            unreachable!("Receiver rejected above")
        };
        let field_ident = match &*pt.pat {
            Pat::Ident(id) => id.ident.clone(),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "props args must be plain `name: Type` identifiers (no patterns)",
                ));
            }
        };
        let ty = &pt.ty;
        prop_fields.push(quote! { pub #field_ident: #ty });
        prop_field_idents.push(field_ident);
    }

    let generics = &sig.generics;
    let where_clause = &generics.where_clause;

    // The user's body, attributed to the renamed inner fn. Splice the FULL signature (vis,
    // generics, fn-ness, args, return type) so users can use generics, async, etc.
    let inner_fn = ItemFn {
        attrs: item.attrs.clone(),
        vis: vis.clone(),
        sig: Signature {
            ident: inner_ident.clone(),
            ..sig.clone()
        },
        block: item.block.clone(),
    };

    // Build the wrapper's call args: ui passes through by name; props come from `props.<field>`;
    // children passes through by name.
    let ui_pat = unpack_pat_ident(first)?;
    let children_pat = unpack_pat_ident(last)?;
    let call_args = std::iter::once(quote! { #ui_pat })
        .chain(prop_field_idents.iter().map(|id| quote! { props.#id }))
        .chain(std::iter::once(quote! { #children_pat }));
    let output = &sig.output;

    let struct_def = if prop_fields.is_empty() {
        quote! {
            #[derive(Default, Clone, Debug)]
            struct #props_ident;
        }
    } else {
        quote! {
            #[derive(Default, Clone, Debug)]
            struct #props_ident {
                #(#prop_fields),*
            }
        }
    };

    Ok(quote! {
        #inner_fn

        #struct_def

        #vis fn #fn_name #generics (
            #ui_pat: &mut egui::Ui,
            props: #props_ident,
            #children_pat: impl FnOnce(&mut egui::Ui),
        ) #output #where_clause {
            #inner_ident(#(#call_args),*)
        }
    })
}

// Unpack the pattern of a Typed arg as an Ident (for ui/children pass-through).
fn unpack_pat_ident(arg: &syn::FnArg) -> Result<syn::Ident, syn::Error> {
    use syn::{FnArg, Pat};
    let FnArg::Typed(pt) = arg else {
        return Err(syn::Error::new_spanned(arg, "self args unsupported"));
    };
    match &*pt.pat {
        Pat::Ident(id) => Ok(id.ident.clone()),
        other => Err(syn::Error::new_spanned(
            other,
            "expected a plain identifier arg",
        )),
    }
}

// snake_case -> PascalCase. `xp_button` -> `XpButton`, `xp_group_box` -> `XpGroupBox`. Splits on
// `_`, capitalizes the first letter of each segment, drops the underscores. Single-word names just
// get the first letter capitalized. Empty input stays empty.
fn to_pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap_next = true;
    for ch in s.chars() {
        if ch == '_' {
            cap_next = true;
            continue;
        }
        if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// Read + parse the stylesheet (if any) and compute per-element styles keyed by AST pointer.
// Errors resolve to a compile_error!() token stream aimed at the header span.
fn build_ctx(parsed: &RsxInput) -> Result<StyleCtx, TokenStream2> {
    let Some(header) = &parsed.css else {
        return Ok(StyleCtx { styles: None });
    };

    // CARGO_MANIFEST_DIR here is the CALLER crate's dir (cargo sets it on the rustc process that
    // runs this proc macro), so the path resolves against demo/, not macro/.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        quote_spanned! { header.span =>
            compile_error!("egui_rsx!: CARGO_MANIFEST_DIR unset; cannot resolve the stylesheet path");
        }
    })?;
    let full = std::path::Path::new(&manifest).join(&header.path);
    let css = std::fs::read_to_string(&full).map_err(|e| {
        let msg = format!(
            "egui_rsx!: cannot read stylesheet `{}`: {e}",
            full.display()
        );
        quote_spanned! { header.span => compile_error!(#msg); }
    })?;

    Ok(StyleCtx {
        styles: Some(compute_styles(&css, &parsed.body.body.roots)),
    })
}

// Wrap the emitted statements in a plausible fn so they parse as a syn::File, then prettyplease
// them. Purely for the checked-in expanded.rs; never runs during normal expansion. The stylesheet
// stem becomes the fn suffix (`view_xp_1`, `view_flex_1`, `view_styles_1`) and a process-local
// counter disambiguates same-stem dumps so the file is always valid Rust (no duplicate fn names).
// `styles` stem owns the file: first call truncates + writes the regen banner; everything else
// appends.
fn dump_expansion(expanded: &TokenStream2, path: &std::ffi::OsStr, stem: &str) {
    use std::sync::{LazyLock, Mutex};
    static COUNTS: LazyLock<Mutex<HashMap<String, usize>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let n = {
        let mut map = COUNTS.lock().unwrap();
        let slot = map.entry(stem.to_string()).or_insert(0);
        let cur = *slot;
        *slot += 1;
        cur
    };
    let fn_name = format!("view_{}_{}", stem, n + 1);
    let fn_ident = Ident::new(&fn_name, proc_macro2::Span::call_site());
    let wrapped = quote! {
        fn #fn_ident(ui: &mut egui::Ui) {
            #expanded
        }
    };
    if let Ok(file) = syn::parse2::<syn::File>(wrapped) {
        let text = prettyplease::unparse(&file);
        let is_first_styles = stem == "styles" && n == 0;
        if is_first_styles {
            const BANNER: &str = "\
// GENERATED -- real macro expansion of every STYLED egui_rsx! block in demo/src/lib.rs. Regenerate:\n\
//   rm -f expanded.rs && touch demo/src/lib.rs && EGUI_RSX_DUMP=\"$PWD/expanded.rs\" cargo build -p egui-rsx-demo\n\
// Each styled view is dumped as its own fn so the file reads top-to-bottom as one Rust source.\n\
// styles.css views come first (truncate + banner), other stylesheets append. Same-stem dumps get\n\
// a numeric suffix so the file is always valid (no duplicate fn names).\n\n";
            let _ = std::fs::write(path, format!("{BANNER}{text}"));
        } else {
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = f.write_all(b"\n");
                let _ = f.write_all(text.as_bytes());
            }
        }
    }
}

// --- emission --------------------------------------------------------------

// Emit a sequence of sibling nodes as statements against the ambient `ui`.
fn emit_roots(nodes: &[BodyNode], ctx: &StyleCtx) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for node in nodes {
        out.extend(emit_node(node, ctx));
    }
    out
}

fn emit_node(node: &BodyNode, ctx: &StyleCtx) -> TokenStream2 {
    match node {
        BodyNode::Element(el) => emit_element(el, ctx),
        BodyNode::Text(text) => {
            let content = text_tokens(text);
            quote! { ui.label(#content); }
        }
        BodyNode::ForLoop(floop) => emit_for(floop, ctx),
        BodyNode::IfChain(chain) => emit_if(chain, ctx),
        BodyNode::RawExpr(expr) => {
            let expr = &expr.expr;
            quote! { #expr; }
        }
        BodyNode::Component(comp) => emit_component(comp, ctx),
    }
}

// Lab 6: component invocation. `<XpButton label="OK" />` lowers to a plain function call against
// the ambient `ui`, following the convention:
//   fn name: <PascalCase>  ->  snake_case function name  (XpButton  ->  xp_button)
//   props:    <PascalCase> + "Props"                     (XpButton  ->  XpButtonProps)
//   call shape: name_snake(ui, NameProps { ..fields, ..Default::default() }, children_closure)
//
// The Props struct MUST impl Default so unspecified fields fall back. The function's third arg
// receives children as `impl FnOnce(&mut egui::Ui)`; absent children pass `|_ui: &mut egui::Ui| {}`.
// Spread (`<Foo ..some_props />`) replaces `..Default::default()` with `..some_props` -- useful
// for pre-built props or builder patterns. Field values pass through verbatim (literals, exprs,
// event closures); the macro never injects `.into()` so Props field types must match what the user
// wrote (use `&'static str` for string-literal props, not `String`, unless the user writes `.into()`
// themselves via `label: {"OK".to_string()}`).
//
// `key:` is reserved (dioxus reconciliation hook); ignored here since egui-rsx has no runtime
// virtual-dom. `Custom(...)` quoted attribute names (`"data-x": ...`) error -- they cannot map to a
// Rust struct field name.
fn emit_component(comp: &Component, ctx: &StyleCtx) -> TokenStream2 {
    let last_seg = match comp.name.segments.last() {
        Some(s) => s,
        None => {
            return quote_spanned! { comp.name.span() =>
                compile_error!("egui_rsx!: component path is empty");
            };
        }
    };
    let pascal = last_seg.ident.to_string();
    let snake = to_snake_case(&pascal);
    let fn_ident = Ident::new(&snake, last_seg.ident.span());
    let props_ident = Ident::new(&format!("{pascal}Props"), last_seg.ident.span());

    // Fold the explicit `name: value` fields into struct-literal initializers.
    let mut field_tokens = TokenStream2::new();
    for attr in &comp.fields {
        if attr.name.is_likely_key() {
            continue;
        }
        match (&attr.name, &attr.value) {
            (AttributeName::BuiltIn(name), value) => {
                let v = attr_value_tokens(value);
                field_tokens.extend(quote! { #name: #v, });
            }
            (AttributeName::Custom(s), _) => {
                return quote_spanned! { s.span() =>
                    compile_error!(
                        "egui_rsx!: quoted attribute names are not supported on components \
                        (use a plain identifier so it maps to a Props struct field)"
                    );
                };
            }
            (AttributeName::Spread(_), _) => {
                unreachable!("dioxus-rsx routes `..expr` into Component::spreads, not fields")
            }
        }
    }

    // Spread replaces Default as the fallback for unspecified fields. First spread wins (dioxus
    // rejects additional spreads at parse time; comp.spreads[1..] would already have errored).
    let spread = match comp.spreads.first() {
        Some(s) => {
            let expr = &s.expr;
            quote! { ..#expr }
        }
        None => quote! { ..::core::default::Default::default() },
    };

    // Children -> trailing closure. Empty children emit an explicit no-op. The closure parameter
    // MUST be named `ui` (not `_ui`): the children stmts the macro emits all reference the ambient
    // `ui`, so the parameter has to shadow it. Otherwise the closure captures the outer `ui` and
    // conflicts with the `name(ui, ...)` call that owns the same borrow. The type annotation makes
    // inference resolve cleanly against the component's third-arg `impl FnOnce(&mut egui::Ui)`.
    let children_body = if comp.children.roots.is_empty() {
        quote! {}
    } else {
        emit_roots(&comp.children.roots, ctx)
    };

    quote! {
        #fn_ident(ui, #props_ident { #field_tokens #spread }, |ui: &mut egui::Ui| {
            #children_body
        });
    }
}

// AttrValue -> tokens for a struct-literal field. Every variant passes through verbatim; no
// `.into()` is injected. `Shorthand(ident)` is dioxus's bare-ident form (`disabled,`); emit the
// ident itself as the value, mirroring `disabled: disabled`.
fn attr_value_tokens(v: &AttributeValue) -> TokenStream2 {
    match v {
        AttributeValue::AttrLiteral(lit) => lit.to_token_stream(),
        AttributeValue::AttrExpr(expr) => expr.to_token_stream(),
        AttributeValue::EventTokens(closure) => closure.to_token_stream(),
        AttributeValue::Shorthand(ident) => ident.to_token_stream(),
        AttributeValue::IfExpr(ifexpr) => ifexpr.to_token_stream(),
    }
}

// PascalCase -> snake_case. "XpButton" -> "xp_button", "HTMLParser" -> "h_t_m_l_parser" (acceptable
// for the lab; component names tend to be CamelCase not acronym-heavy). Single-segment names only;
// paths (`a::b::Comp`) keep the last segment's ident.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn emit_element(el: &Element, ctx: &StyleCtx) -> TokenStream2 {
    let styled = ctx.get(el);
    match element_name(el).as_str() {
        "div" => {
            let class = class_markers(el);
            let children = emit_roots(&el.children, ctx);
            // Lab 5 slice 3: emit ::before pseudo at the top of the content area (block-flow
            // stacking). Pulled out before the styled/unstyled branch so both paths support it.
            let before_paint = styled
                .and_then(|s| s.before.as_ref().map(|p| emit_pseudo(p)))
                .unwrap_or_default();
            let after_paint = styled
                .and_then(|s| s.after.as_ref().map(|p| emit_pseudo(p)))
                .unwrap_or_default();
            let Some(s) = styled else {
                // Unstyled (Lab 1): `Frame::group`, byte-identical output.
                return quote! {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        #class
                        #before_paint
                        #children
                        #after_paint
                    });
                };
            };
            // Lab 4: `display: flex` lowers to egui_flex instead of a plain Frame.
            if s.base.display_flex {
                return emit_flex(el, s, ctx);
            }
            let frame = container_frame(&s.base);
            let (inset, outset, reasons) = split_shadow(&s.base.box_shadow);
            let has_gradient = s.base.gradient.is_some();
            let has_image = s.base.background_image.is_some();
            let need_behind = has_gradient || !outset.is_empty();
            // Lab 5: honor explicit width/height on a plain (non-flex) div. egui's Frame sizes to
            // content, so an empty div would be 0x0 and the background-image would paint nothing.
            // set_min_size on the inner ui forces the Frame to wrap a sized cell.
            let set_size = match (s.base.width, s.base.height) {
                (Some(Len::Points(w)), Some(Len::Points(h))) => {
                    quote! { ui.set_min_size(egui::vec2(#w, #h)); }
                }
                (Some(Len::Points(w)), None) => {
                    quote! { ui.set_min_width(#w); }
                }
                (None, Some(Len::Points(h))) => {
                    quote! { ui.set_min_height(#h); }
                }
                _ => quote! {},
            };
            // Fast path: a plain styled Frame (Lab 2 shape) when no gradient/shadow/image/size is
            // present, so screenshots 04/05 stay byte-identical. (before/after pseudos still need
            // the slow path because they reserve their own paint slots inside the closure.)
            if !need_behind
                && !has_image
                && inset.is_empty()
                && reasons.is_empty()
                && set_size.is_empty()
                && before_paint.is_empty()
                && after_paint.is_empty()
            {
                return quote! {
                    #frame.show(ui, |ui| {
                        #class
                        #children
                    });
                };
            }
            // A gradient/outset background is painted BEHIND: reserve a paint slot before the frame,
            // fill it after (so it sits under the frame + children). Inset bevels paint ON TOP of
            // the allocated rect after the frame returns. Lab 5: a background-image reserves a
            // SECOND slot (so it can sit under the children but over the gradient/outset mesh).
            let reserve_bg = if need_behind {
                quote! { let __bg = ui.painter().add(egui::Shape::Noop); }
            } else {
                quote! {}
            };
            let reserve_img = if has_image {
                quote! { let __img_slot = ui.painter().add(egui::Shape::Noop); }
            } else {
                quote! {}
            };
            let behind = if need_behind {
                behind_mesh(&s.base.gradient, &outset)
            } else {
                quote! {}
            };
            let image_paint = if has_image {
                texture_paint(&s.base.background_image)
            } else {
                quote! {}
            };
            let inset_paint = bevel_calls(&inset);
            let unsupported = inert_bindings(&reasons);
            quote! {
                {
                    #reserve_bg
                    #reserve_img
                    let __ir = #frame.show(ui, |ui| {
                        #set_size
                        #class
                        #before_paint
                        #children
                        #after_paint
                    });
                    let __r = __ir.response.rect;
                    #behind
                    #image_paint
                    #inset_paint
                    #unsupported
                }
            }
        }
        "button" => emit_button(el, styled),
        "label" => emit_label(el, styled),
        "checkbox" => emit_checkbox(el),
        "textinput" => emit_textinput(el),
        "slider" => emit_slider(el),
        other => {
            let msg = format!(
                "egui_rsx!: unsupported element `{other}`; supports div/button/label/checkbox/textinput/slider"
            );
            quote_spanned! { el.name.span() => compile_error!(#msg); }
        }
    }
}

// Lab 8: signal-bound form widgets. Each takes a value attribute (READ) + an `onchange` closure
// (WRITE). The macro emits:
//   1. a temp `let mut __v = <value expr>;`
//   2. `let __r = ui.<widget>(&mut __v, ...);` (the widget mutates __v in place)
//   3. `if __r.changed() { (<onchange closure>)(__v); }` (closure takes the new value, fires once)
//
// The closure has signature `Fn(T)` -- NOT `Fn()`. So `onchange: move |v| signal.set(v)` works.
// This is the signal-binding pattern: read from a Mutable via `.get()` in the value attribute,
// write via `.set(v)` in the onchange closure. Mutable<T> is Clone so each widget captures its
// own setter -- sidesteps the `&mut Vec<Intent>` reborrow limit that onclick-on-buttons hits.
//
// CSS does not style these widgets; they use egui's default visuals scoped per-widget (the lab
// demo's egui_kittest harness uses wgpu + egui defaults, no XP theme). Threading CSS through
// checkbox/slider visuals would need a `ui.visuals_mut()` scope like emit_button does -- left
// for a later pass since form widgets in the demo don't need to look XP.

// Helper: extract a closure-valued attribute (onchange, oninput, etc.). Same shape as `onclick`
// extraction but parameterized by name. Returns the closure tokens (PartialClosure.to_token_stream)
// so the macro can emit `(<closure>)(<value>)`.
fn event_closure(el: &Element, name: &str) -> Option<TokenStream2> {
    el.raw_attributes
        .iter()
        .find_map(|attr| match (&attr.name, &attr.value) {
            (AttributeName::BuiltIn(id), AttributeValue::EventTokens(closure)) if *id == name => {
                Some(closure.to_token_stream())
            }
            _ => None,
        })
}

// Helper: extract an arbitrary expr-valued attribute (`checked: {expr}`, `value: {expr}`,
// `min: 0`, etc.). Returns the expr tokens. For AttrLiteral (bare number/string) the literal
// tokens are returned; for AttrExpr (the `{expr}` form) the inner PartialExpr's tokens. Used
// for the value side of form widgets.
fn expr_attr(el: &Element, name: &str) -> Option<TokenStream2> {
    el.raw_attributes
        .iter()
        .find_map(|attr| match (&attr.name, &attr.value) {
            (AttributeName::BuiltIn(id), value) if *id == name => Some(attr_value_tokens(value)),
            _ => None,
        })
}

// <checkbox checked: {bool_expr} onchange: move |v| setter(v) "Label text" />
// Optional `label` text node as the third arg. Falls back to `ui.checkbox(&mut v, label)`.
fn emit_checkbox(el: &Element) -> TokenStream2 {
    let checked = match expr_attr(el, "checked") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: checkbox requires `checked: {expr}` attribute");
            };
        }
    };
    let onchange = match event_closure(el, "onchange") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: checkbox requires `onchange: \\|v\\| ...` closure");
            };
        }
    };
    let label = first_text(el)
        .map(|t| text_tokens(t))
        .unwrap_or_else(|| quote! { "" });
    quote! {
        {
            let mut __v: bool = (#checked);
            let __r = ui.checkbox(&mut __v, #label);
            if __r.changed() {
                (#onchange)(__v);
            }
        }
    }
}

// <textinput value: {string_expr} onchange: move |s| setter(s) />
// Optional `placeholder`, `password` (bool), `multiline` (bool). Single-line by default.
// The value is read into a String, mutated by TextEdit, and the new String is the onchange arg.
fn emit_textinput(el: &Element) -> TokenStream2 {
    let value = match expr_attr(el, "value") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: textinput requires `value: {expr}` attribute");
            };
        }
    };
    let onchange = match event_closure(el, "onchange") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: textinput requires `onchange: \\|s\\| ...` closure");
            };
        }
    };
    let placeholder = expr_attr(el, "placeholder").unwrap_or_else(|| quote! { "" });
    let password = expr_attr(el, "password").unwrap_or_else(|| quote! { false });
    let multiline = expr_attr(el, "multiline").unwrap_or_else(|| quote! { false });
    let desired_width = expr_attr(el, "width").unwrap_or_else(|| quote! { 160.0 });
    quote! {
        {
            let mut __s: String = (#value);
            let mut __te = egui::TextEdit::singleline(&mut __s)
                .desired_width(#desired_width)
                .hint_text(#placeholder)
                .password(#password);
            if #multiline {
                __te = egui::TextEdit::multiline(&mut __s)
                    .desired_width(#desired_width)
                    .hint_text(#placeholder)
                    .password(#password);
            }
            let __r = ui.add(__te);
            if __r.changed() {
                (#onchange)(__s);
            }
        }
    }
}

// <slider value: {number_expr} min: 0 max: 10 onchange: move |v| setter(v) />
// Integer or float; the macro emits `Slider::new(&mut v, range).` If `step` is present, calls
// `.step_by(step)`. The value's type is inferred from the expr -- i32 or f32 are both supported
// by egui's Slider (via the Num trait).
fn emit_slider(el: &Element) -> TokenStream2 {
    let value = match expr_attr(el, "value") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: slider requires `value: {expr}` attribute");
            };
        }
    };
    let onchange = match event_closure(el, "onchange") {
        Some(t) => t,
        None => {
            return quote_spanned! { el.name.span() =>
                compile_error!("egui_rsx!: slider requires `onchange: \\|v\\| ...` closure");
            };
        }
    };
    let min = expr_attr(el, "min").unwrap_or_else(|| quote! { 0.0 });
    let max = expr_attr(el, "max").unwrap_or_else(|| quote! { 1.0 });
    let step = expr_attr(el, "step");
    let text = first_text(el)
        .map(|t| text_tokens(t))
        .unwrap_or_else(|| quote! { "" });
    let step_call = step
        .map(|s| quote! { .step_by(#s) })
        .unwrap_or_else(|| quote! {});
    quote! {
        {
            let mut __v = (#value);
            let __r = ui.add(
                egui::Slider::new(&mut __v, #min..=#max)
                    #step_call
                    .text(#text)
            );
            if __r.changed() {
                (#onchange)(__v);
            }
        }
    }
}

// Lab 4: lower a `display: flex` div to an `egui_flex::Flex`. The container's visual Frame
// (background/border/padding from Lab 2 + gradient/shadow from Lab 3) wraps the Flex; each
// child becomes one flex item via `add_ui`. Layout is runtime-resolved by egui_flex against
// the available rect, so it can't be baked as a literal the way colors/strokes are.
fn emit_flex(el: &Element, s: &ElemStyles, ctx: &StyleCtx) -> TokenStream2 {
    let class = class_markers(el);
    let children = emit_flex_children(&el.children, ctx);
    let frame = container_frame(&s.base);
    let flex = build_flex(&s.base);

    // Reuse the Lab 3 gradient/outset paint-behind + inset bevel machinery. The flex layout
    // sits inside the Frame, so the Frame's rect is still the paint target for both. Lab 5 slice
    // 2: also wire background-image (texture_paint), so a nested-flex div with `background-image`
    // gets its SVG glyph painted. The image slot is reserved AFTER the bg slot and BEFORE the
    // frame.show, so it paints over the gradient/outset but under the frame fill + flex children.
    let (inset, outset, reasons) = split_shadow(&s.base.box_shadow);
    let has_gradient = s.base.gradient.is_some();
    let has_image = s.base.background_image.is_some();
    let need_behind = has_gradient || !outset.is_empty();
    let reserve_bg = if need_behind {
        quote! { let __bg = ui.painter().add(egui::Shape::Noop); }
    } else {
        quote! {}
    };
    let reserve_img = if has_image {
        quote! { let __img_slot = ui.painter().add(egui::Shape::Noop); }
    } else {
        quote! {}
    };
    let behind = if need_behind {
        behind_mesh(&s.base.gradient, &outset)
    } else {
        quote! {}
    };
    let image_paint = if has_image {
        texture_paint(&s.base.background_image)
    } else {
        quote! {}
    };
    let inset_paint = bevel_calls(&inset);
    let unsupported = inert_bindings(&reasons);

    quote! {
        {
            #reserve_bg
            #reserve_img
            let __ir = #frame.show(ui, |ui| {
                #class
                #flex.show(ui, |__flex| {
                    #children
                });
            });
            let __r = __ir.response.rect;
            #behind
            #image_paint
            #inset_paint
            #unsupported
        }
    }
}

// Build the `egui_flex::Flex` container from the baked layout props. Every value is a literal.
fn build_flex(s: &ComputedStyle) -> TokenStream2 {
    let mut f = quote! { egui_flex::Flex::new() };
    if s.flex_column {
        f = quote! { #f.direction(egui_flex::FlexDirection::Vertical) };
    }
    if let Some(g) = s.gap {
        f = quote! { #f.gap(egui::vec2(#g, #g)) };
    }
    // CSS `display: flex` defaults `width: auto` to fill the available width (block-level).
    // egui_flex defaults to `w_auto` (content-sized) which leaves no slack for flex-grow, so an
    // unset width defaults to `w_full()` to match CSS. Explicit width (points/percent) wins.
    match s.width {
        Some(Len::Points(p)) => f = quote! { #f.width(egui_flex::Size::Points(#p)) },
        Some(Len::Percent(p)) => f = quote! { #f.width(egui_flex::Size::Percent(#p)) },
        None => f = quote! { #f.w_full() },
    }
    if let Some(l) = s.height {
        f = match l {
            Len::Points(p) => quote! { #f.height(egui_flex::Size::Points(#p)) },
            Len::Percent(p) => quote! { #f.height(egui_flex::Size::Percent(#p)) },
        };
    }
    if let Some(j) = s.justify {
        let tok = match j {
            FlexJustify::Start => quote! { egui_flex::FlexJustify::Start },
            FlexJustify::End => quote! { egui_flex::FlexJustify::End },
            FlexJustify::Center => quote! { egui_flex::FlexJustify::Center },
            FlexJustify::SpaceBetween => quote! { egui_flex::FlexJustify::SpaceBetween },
            FlexJustify::SpaceAround => quote! { egui_flex::FlexJustify::SpaceAround },
            FlexJustify::SpaceEvenly => quote! { egui_flex::FlexJustify::SpaceEvenly },
        };
        f = quote! { #f.justify(#tok) };
    }
    if let Some(a) = s.align {
        let tok = match a {
            FlexAlign::Start => quote! { egui_flex::FlexAlign::Start },
            FlexAlign::End => quote! { egui_flex::FlexAlign::End },
            FlexAlign::Center => quote! { egui_flex::FlexAlign::Center },
            FlexAlign::Stretch => quote! { egui_flex::FlexAlign::Stretch },
        };
        f = quote! { #f.align_items(#tok) };
    }
    f
}

// Emit each child of a flex container. The egui_flex-idiomatic shape: a `div` child's visual
// Frame (bg/border/padding) goes on `FlexItem::frame(...)` so egui_flex draws it AT the cell
// rect, which stretches with `align-items: stretch`. Wrapping the Frame INSIDE the closure would
// shrink to content and defeat the stretch.
//
// A `div` child that is itself `display: flex` recurses via `add_flex` so its own flex layout
// runs (nested flex). Otherwise its children draw inside the cell's content rect via `add_ui`.
//
// Lab 5: a flex-child div with `background-image: svg-load(...)` paints its texture INSIDE the
// `add_ui` closure against the cell rect (`ui.max_rect()` at closure start). Reserve a slot
// before the children draw, fill it after — same z-ordering trick emit_element uses around
// Frame::show. (Gradient + box-shadow still dropped here; the FlexItem::frame owns the cell's
// visual Frame and we can't insert a paint-behind between it and the children without an egui_flex
// API change. Documented as a Lab 5 limit.)
fn emit_flex_children(children: &[BodyNode], ctx: &StyleCtx) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for child in children {
        match child {
            BodyNode::Element(el) if element_name(el) == "div" => {
                let styled = ctx.get(el);
                let mut item = match styled {
                    Some(s) => {
                        let f = container_frame(&s.base);
                        quote! { egui_flex::item().frame(#f) }
                    }
                    None => quote! { egui_flex::item() },
                };
                if let Some(s) = styled {
                    if let Some(g) = s.base.flex_grow {
                        item = quote! { #item.grow(#g) };
                    }
                }
                let nested_flex = styled.map(|s| s.base.display_flex).unwrap_or(false);
                if nested_flex {
                    let s = styled.expect("display_flex implies styled");
                    let inner = build_flex(&s.base);
                    let kids = emit_flex_children(&el.children, ctx);
                    out.extend(quote! {
                        __flex.add_flex(#item, #inner, |__flex| { #kids });
                    });
                } else {
                    let class = class_markers(el);
                    let kids = emit_roots(&el.children, ctx);
                    let has_image = styled
                        .map(|s| s.base.background_image.is_some())
                        .unwrap_or(false);
                    if has_image {
                        // Reserve a paint slot before children draw (slot sits UNDER them in
                        // z-order), then fill it after children with the textured mesh against
                        // the cell rect. Texture paint reuses the emit_element texture_paint
                        // helper; the `__r` it expects is the cell's max_rect at closure entry.
                        let img = texture_paint(&styled.unwrap().base.background_image);
                        out.extend(quote! {
                            __flex.add_ui(#item, |ui| {
                                let __r = ui.max_rect();
                                let __img_slot = ui.painter().add(egui::Shape::Noop);
                                #class
                                #kids
                                #img
                            });
                        });
                    } else {
                        out.extend(quote! {
                            __flex.add_ui(#item, |ui| {
                                #class
                                #kids
                            });
                        });
                    }
                }
            }
            other => {
                let body = emit_node(other, ctx);
                out.extend(quote! {
                    __flex.add_ui(egui_flex::item(), |ui| { #body });
                });
            }
        }
    }
    out
}

fn emit_label(el: &Element, styled: Option<&ElemStyles>) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for text in text_children(el) {
        let content = text_tokens(text);
        let widget = match styled {
            Some(s) => rich_text(&content, &s.base),
            None => content,
        };
        out.extend(quote! { ui.label(#widget); });
    }
    out
}

fn emit_button(el: &Element, styled: Option<&ElemStyles>) -> TokenStream2 {
    let label = first_text(el)
        .map(|t| text_tokens(t))
        .unwrap_or_else(|| quote! { "" });
    let handler = onclick(el);

    match styled {
        None => match handler {
            Some(h) => quote! { if ui.button(#label).clicked() { (#h)(); } },
            None => quote! { ui.button(#label); },
        },
        Some(s) => {
            // Button fill/border/hover/active -> a SCOPED visuals override (widgets.{inactive,
            // hovered,active}). Text color -> RichText. Restore prior visuals after the call so
            // nothing leaks to sibling widgets. No runtime cascade — every value is a literal.
            let mut overrides = button_visuals(s);
            let btn_label = match s.base.color {
                Some(c) => {
                    let c = color_tokens(c);
                    quote! { egui::RichText::new(#label).color(#c) }
                }
                None => quote! { egui::RichText::new(#label) },
            };
            let click = match handler {
                Some(h) => quote! { if __resp.clicked() { (#h)(); } },
                None => quote! { let _ = __resp; },
            };

            // box-shadow bevels, selected by the button's runtime interaction state (the CSS
            // :active/:hover cascade already baked distinct stacks into s.active/s.hover/s.base).
            let (base_i, base_o, mut reasons) = split_shadow(&s.base.box_shadow);
            let (hover_i, _, _) = split_shadow(&s.hover.box_shadow);
            let (active_i, _, _) = split_shadow(&s.active.box_shadow);
            let has_shadow = !base_i.is_empty() || !hover_i.is_empty() || !active_i.is_empty();
            if !base_o.is_empty() {
                reasons.push(
                    "outset box-shadow on button unsupported: no paint-behind slot for a widget"
                        .to_string(),
                );
            }
            if s.base.gradient.is_some() {
                reasons.push(
                    "gradient background on button unsupported: use a solid background-color"
                        .to_string(),
                );
            }

            let bevel = if has_shadow {
                // Suppress egui's own button frame (stroke/rounding/grow) so only our bevel shows.
                overrides.extend(quote! {
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
                    ui.visuals_mut().widgets.inactive.expansion = 0.0;
                    ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
                    ui.visuals_mut().widgets.hovered.expansion = 0.0;
                    ui.visuals_mut().widgets.active.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::ZERO;
                    ui.visuals_mut().widgets.active.expansion = 0.0;
                });
                let base_b = bevel_calls(&base_i);
                let hover_b = bevel_calls(&hover_i);
                let active_b = bevel_calls(&active_i);
                quote! {
                    let __r = __resp.rect;
                    if __resp.is_pointer_button_down_on() {
                        #active_b
                    } else if __resp.hovered() {
                        #hover_b
                    } else {
                        #base_b
                    }
                }
            } else {
                quote! {}
            };
            let unsupported = inert_bindings(&reasons);

            quote! {
                {
                    let __prev_visuals = ui.visuals().clone();
                    #overrides
                    let __resp = ui.button(#btn_label);
                    #bevel
                    #click
                    *ui.visuals_mut() = __prev_visuals;
                    #unsupported
                }
            }
        }
    }
}

fn emit_for(floop: &ForLoop, ctx: &StyleCtx) -> TokenStream2 {
    let pat = &floop.pat;
    let expr = &floop.expr;
    let body = emit_roots(&floop.body.roots, ctx);
    quote! { for #pat in #expr { #body } }
}

fn emit_if(chain: &IfChain, ctx: &StyleCtx) -> TokenStream2 {
    let cond = &chain.cond;
    let then = emit_roots(&chain.then_branch.roots, ctx);
    let head = quote! { if #cond { #then } };

    if let Some(elif) = &chain.else_if_branch {
        let inner = emit_if(elif, ctx);
        quote! { #head else #inner }
    } else if let Some(else_branch) = &chain.else_branch {
        let els = emit_roots(&else_branch.roots, ctx);
        quote! { #head else { #els } }
    } else {
        head
    }
}

// --- property -> egui mapping ----------------------------------------------

fn container_frame(s: &ComputedStyle) -> TokenStream2 {
    let mut f = quote! { egui::Frame::new() };
    // A gradient background is painted as a Mesh behind the frame, so skip Frame::fill here.
    if let Some(c) = s.background_color {
        if s.gradient.is_none() {
            let c = color_tokens(c);
            f = quote! { #f.fill(#c) };
        }
    }
    if s.border_width.is_some() || s.border_color.is_some() {
        let w = s.border_width.unwrap_or(1.0);
        let c = color_tokens(s.border_color.unwrap_or((0, 0, 0, 255)));
        f = quote! { #f.stroke(egui::Stroke::new(#w, #c)) };
    }
    if let Some(r) = s.border_radius {
        let r = r.round() as u8;
        f = quote! { #f.corner_radius(egui::CornerRadius::same(#r)) };
    }
    if let Some(p) = s.padding {
        let p = p.round() as i8;
        f = quote! { #f.inner_margin(egui::Margin::same(#p)) };
    }
    f
}

// Label/button text -> RichText with color/size/background. Bare content when nothing applies
// (keeps the unstyled path a plain `ui.label(content)`).
fn rich_text(content: &TokenStream2, s: &ComputedStyle) -> TokenStream2 {
    if s.color.is_none() && s.font_size.is_none() && s.background_color.is_none() {
        return content.clone();
    }
    let mut rt = quote! { egui::RichText::new(#content) };
    if let Some(c) = s.color {
        let c = color_tokens(c);
        rt = quote! { #rt.color(#c) };
    }
    if let Some(sz) = s.font_size {
        rt = quote! { #rt.size(#sz) };
    }
    if let Some(bg) = s.background_color {
        let bg = color_tokens(bg);
        rt = quote! { #rt.background_color(#bg) };
    }
    rt
}

// egui uses `weak_bg_fill` for optional-fill widgets like buttons; set `bg_fill` too as a belt.
fn button_visuals(s: &ElemStyles) -> TokenStream2 {
    let mut out = TokenStream2::new();
    if let Some(c) = s.base.background_color {
        let c = color_tokens(c);
        out.extend(quote! {
            ui.visuals_mut().widgets.inactive.weak_bg_fill = #c;
            ui.visuals_mut().widgets.inactive.bg_fill = #c;
        });
    }
    if let Some(r) = s.base.border_radius {
        let r = r.round() as u8;
        out.extend(quote! {
            ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::same(#r);
            ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::same(#r);
            ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::same(#r);
        });
    }
    if let Some(c) = s.hover.background_color {
        let c = color_tokens(c);
        out.extend(quote! {
            ui.visuals_mut().widgets.hovered.weak_bg_fill = #c;
            ui.visuals_mut().widgets.hovered.bg_fill = #c;
        });
    }
    if let Some(c) = s.active.background_color {
        let c = color_tokens(c);
        out.extend(quote! {
            ui.visuals_mut().widgets.active.weak_bg_fill = #c;
            ui.visuals_mut().widgets.active.bg_fill = #c;
        });
    }
    out
}

fn color_tokens((r, g, b, a): Color) -> TokenStream2 {
    quote! { egui::Color32::from_rgba_unmultiplied(#r, #g, #b, #a) }
}

// --- Lab 3: box-shadow + gradient lowering ---------------------------------

// Partition a box-shadow stack into what we can paint and what we cannot. inset+blur0 -> painted
// INSIDE the rect (bevel edges); non-inset+blur0 -> painted BEHIND/outside; blur>0 -> unsupported
// (an inert `let` binding with a reason, the Lab 2 convention). Returns (inset, outset, reasons).
fn split_shadow(
    layers: &Option<Vec<ShadowLayer>>,
) -> (Vec<ShadowLayer>, Vec<ShadowLayer>, Vec<String>) {
    let mut inset = Vec::new();
    let mut outset = Vec::new();
    let mut reasons = Vec::new();
    let Some(layers) = layers else {
        return (inset, outset, reasons);
    };
    for l in layers {
        if l.blur != 0.0 {
            reasons.push(format!(
                "box-shadow blur ({}px) unsupported: egui has no blurred rect painter",
                l.blur
            ));
            continue;
        }
        if l.spread != 0.0 {
            reasons.push(format!(
                "box-shadow spread ({}px) unsupported: bevel lowering is offset-only",
                l.spread
            ));
            continue;
        }
        if l.inset {
            inset.push(l.clone());
        } else {
            outset.push(l.clone());
        }
    }
    (inset, outset, reasons)
}

// Paint inset bevel bands over `__r` (must be in scope). Each inset layer exposes a band of width
// |ox| on the left(ox>0)/right(ox<0) edge and |oy| on the top(oy>0)/bottom(oy<0) edge. CSS paints
// the FIRST-declared layer on top; egui paints last-submitted on top, so we submit in REVERSE
// declaration order. spread is ignored (offset-only bevels; noted in Status).
fn bevel_calls(layers: &[ShadowLayer]) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for l in layers.iter().rev() {
        let c = color_tokens(l.color);
        let (ox, oy) = (l.ox, l.oy);
        if ox > 0.0 {
            out.extend(quote! {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(__r.left(), __r.top()),
                        egui::pos2(__r.left() + #ox, __r.bottom())),
                    egui::CornerRadius::ZERO, #c);
            });
        } else if ox < 0.0 {
            out.extend(quote! {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(__r.right() + #ox, __r.top()),
                        egui::pos2(__r.right(), __r.bottom())),
                    egui::CornerRadius::ZERO, #c);
            });
        }
        if oy > 0.0 {
            out.extend(quote! {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(__r.left(), __r.top()),
                        egui::pos2(__r.right(), __r.top() + #oy)),
                    egui::CornerRadius::ZERO, #c);
            });
        } else if oy < 0.0 {
            out.extend(quote! {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(__r.left(), __r.bottom() + #oy),
                        egui::pos2(__r.right(), __r.bottom())),
                    egui::CornerRadius::ZERO, #c);
            });
        }
    }
    out
}

// Build the behind-the-content Mesh (`__mesh`) from a gradient (per-vertex colored quads) plus any
// outset blur-0 shadows (solid rects offset outside `__r`), then drop it into the reserved `__bg`
// slot. Assumes `__r`, `__bg`, `ui` in scope.
fn behind_mesh(gradient: &Option<Gradient>, outset: &[ShadowLayer]) -> TokenStream2 {
    let mut body = TokenStream2::new();
    for l in outset {
        let c = color_tokens(l.color);
        let (ox, oy) = (l.ox, l.oy);
        body.extend(quote! {
            __mesh.add_colored_rect(
                egui::Rect::from_min_max(
                    egui::pos2(__r.left() + #ox, __r.top() + #oy),
                    egui::pos2(__r.right() + #ox, __r.bottom() + #oy)),
                #c);
        });
    }
    if let Some(g) = gradient {
        body.extend(gradient_quads(g));
    }
    quote! {
        let mut __mesh = egui::Mesh::default();
        #body
        ui.painter().set(__bg, __mesh);
    }
}

// Lab 5 slice 3: emit a pseudo-element (::before/::after) as a virtual child div with its own
// Frame, sized via set_min_size to its width/height, painted inside the parent element's content
// area. Block-flow stacking: ::before paints at the top (call BEFORE emitting the parent's
// children), ::after at the bottom (call AFTER). Each pseudo is wrapped in its own `{ ... }`
// block so the inner names (`__bg`, `__img_slot`, `__r`) don't collide with the parent's.
//
// Reuses the Lab 3 + Lab 5 machinery (gradient/outset mesh behind, background-image texture,
// inset bevels on top). content: "..." is parsed but the literal text is currently dropped
// (egui labels would need to draw inside the pseudo's Frame; out of MVP scope).
fn emit_pseudo(style: &ComputedStyle) -> TokenStream2 {
    let frame = container_frame(style);
    let (inset, outset, reasons) = split_shadow(&style.box_shadow);
    let has_gradient = style.gradient.is_some();
    let has_image = style.background_image.is_some();
    let need_behind = has_gradient || !outset.is_empty();
    let set_size = match (style.width, style.height) {
        (Some(Len::Points(w)), Some(Len::Points(h))) => {
            quote! { ui.set_min_size(egui::vec2(#w, #h)); }
        }
        (Some(Len::Points(w)), None) => quote! { ui.set_min_width(#w); },
        (None, Some(Len::Points(h))) => quote! { ui.set_min_height(#h); },
        _ => quote! {},
    };
    // If nothing styles the pseudo, emit nothing (don't even allocate a Frame).
    let has_anything = need_behind
        || has_image
        || !inset.is_empty()
        || !reasons.is_empty()
        || style.background_color.is_some()
        || style.border_width.is_some()
        || style.border_color.is_some()
        || style.border_radius.is_some()
        || style.padding.is_some()
        || !set_size.is_empty();
    if !has_anything {
        return quote! {};
    }
    let reserve_bg = if need_behind {
        quote! { let __bg = ui.painter().add(egui::Shape::Noop); }
    } else {
        quote! {}
    };
    let reserve_img = if has_image {
        quote! { let __img_slot = ui.painter().add(egui::Shape::Noop); }
    } else {
        quote! {}
    };
    let behind = if need_behind {
        behind_mesh(&style.gradient, &outset)
    } else {
        quote! {}
    };
    let image_paint = if has_image {
        texture_paint(&style.background_image)
    } else {
        quote! {}
    };
    let inset_paint = bevel_calls(&inset);
    let unsupported = inert_bindings(&reasons);
    quote! {
        {
            #reserve_bg
            #reserve_img
            let __ir = #frame.show(ui, |ui| {
                #set_size
            });
            let __r = __ir.response.rect;
            #behind
            #image_paint
            #inset_paint
            #unsupported
        }
    }
}

// Lab 5 slice 2: paint a rasterized SVG texture into the reserved `__img_slot`. The bytes are
// baked as a `&[u8]` literal; an `OnceLock<TextureHandle>` static (unique per call site via a
// thread-local counter) caches the upload so the texture is created once on the first frame and
// reused thereafter. The texture paints INSIDE the element rect (stretch-to-fill); the rect comes
// from `Frame::show`'s response, same as the gradient mesh and bevel bands.
//
// z-order: __img_slot is reserved AFTER __bg and BEFORE Frame::show, so egui paints it after the
// gradient/outset mesh (covers it) but before the frame fill + children (children render on top).
fn texture_paint(raster: &Option<Rc<Raster>>) -> TokenStream2 {
    let Some(raster) = raster else {
        return TokenStream2::new();
    };
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tex_name = quote::format_ident!("__egui_rsx_tex_{}", n);
    let bytes_name = quote::format_ident!("__egui_rsx_tex_bytes_{}", n);
    let id_str = format!("egui_rsx_svg_{n}");
    let bytes = &raster.bytes;
    let w = raster.width as usize;
    let h = raster.height as usize;
    quote! {
        // Per-call-site lazy texture handle. First frame: rasterize (already done at macro time),
        // upload bytes. Later frames: hit the cache, no allocation.
        static #tex_name: std::sync::OnceLock<egui::TextureHandle> = std::sync::OnceLock::new();
        let __handle = #tex_name.get_or_init(|| {
            const #bytes_name: &[u8] = &[#(#bytes),*];
            ui.ctx().load_texture(
                #id_str,
                egui::ColorImage::from_rgba_unmultiplied([#w, #h], #bytes_name),
                egui::TextureOptions::LINEAR,
            )
        });
        let mut __mesh = egui::Mesh::with_texture(__handle.id());
        __mesh.add_rect_with_uv(
            __r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        ui.painter().set(__img_slot, egui::Shape::Mesh(std::sync::Arc::new(__mesh)));
    }
}

// Emit the per-segment quad-building statements for a gradient over `__r` into `__mesh`. Vertical
// dirs interpolate color down the y axis (full-width rows); horizontal down the x axis.
fn gradient_quads(g: &Gradient) -> TokenStream2 {
    let pts = gradient_points(g);
    let horizontal = matches!(g.dir, GradientDir::LeftToRight | GradientDir::RightToLeft);
    let mut out = TokenStream2::new();
    for w in pts.windows(2) {
        let (f0, c0) = w[0];
        let (f1, c1) = w[1];
        let c0 = color_tokens(c0);
        let c1 = color_tokens(c1);
        if horizontal {
            out.extend(quote! {
                {
                    let __x0 = __r.left() + #f0 * __r.width();
                    let __x1 = __r.left() + #f1 * __r.width();
                    let __b = __mesh.vertices.len() as u32;
                    __mesh.colored_vertex(egui::pos2(__x0, __r.top()), #c0);
                    __mesh.colored_vertex(egui::pos2(__x1, __r.top()), #c1);
                    __mesh.colored_vertex(egui::pos2(__x1, __r.bottom()), #c1);
                    __mesh.colored_vertex(egui::pos2(__x0, __r.bottom()), #c0);
                    __mesh.add_triangle(__b, __b + 1, __b + 2);
                    __mesh.add_triangle(__b, __b + 2, __b + 3);
                }
            });
        } else {
            out.extend(quote! {
                {
                    let __y0 = __r.top() + #f0 * __r.height();
                    let __y1 = __r.top() + #f1 * __r.height();
                    let __b = __mesh.vertices.len() as u32;
                    __mesh.colored_vertex(egui::pos2(__r.left(), __y0), #c0);
                    __mesh.colored_vertex(egui::pos2(__r.right(), __y0), #c0);
                    __mesh.colored_vertex(egui::pos2(__r.right(), __y1), #c1);
                    __mesh.colored_vertex(egui::pos2(__r.left(), __y1), #c1);
                    __mesh.add_triangle(__b, __b + 1, __b + 2);
                    __mesh.add_triangle(__b, __b + 2, __b + 3);
                }
            });
        }
    }
    out
}

// Unsupported declarations survive as inert `let` bindings (token streams carry no `//` comments),
// visible in expanded.rs, underscore-prefixed so they never warn. The Lab 2 convention, reused.
fn inert_bindings(reasons: &[String]) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for (i, r) in reasons.iter().enumerate() {
        let name = quote::format_ident!("_egui_rsx_unsupported_{}", i);
        out.extend(quote! { let #name = #r; });
    }
    out
}

// --- CSS: parse + compile-time cascade -------------------------------------

type Color = (u8, u8, u8, u8);

#[derive(Default, Clone)]
struct ComputedStyle {
    background_color: Option<Color>,
    color: Option<Color>,
    border_width: Option<f32>,
    border_color: Option<Color>,
    border_radius: Option<f32>,
    padding: Option<f32>,
    font_size: Option<f32>,
    // Lab 3. `None` = property absent (inherit prior cascade); `Some(vec![])` = explicit `none`.
    box_shadow: Option<Vec<ShadowLayer>>,
    gradient: Option<Gradient>,
    // Lab 5 slice 2: rasterized SVG (svg-load). Populated only in `base` (background-image does
    // not vary by :hover/:active in the demo); cheap Rc clone when apply() runs per state.
    background_image: Option<Rc<Raster>>,
    // Lab 4: flexbox layout. Only `base` carries these (layout does not vary on :hover/:active).
    // `display_flex` is the trigger; the rest are read only when it is true.
    display_flex: bool,
    flex_column: bool, // false (default) = row; true = column
    gap: Option<f32>,
    width: Option<Len>,
    height: Option<Len>,
    flex_grow: Option<f32>,
    justify: Option<FlexJustify>,
    align: Option<FlexAlign>,
}

// Lab 4 length: points or percent of the available main/cross size. `auto` parses to `None`.
#[derive(Clone, Copy, PartialEq)]
enum Len {
    Points(f32),
    Percent(f32),
}

#[derive(Clone, Copy, PartialEq)]
enum FlexJustify {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, PartialEq)]
enum FlexAlign {
    Start,
    End,
    Center,
    Stretch,
}

// One CSS `box-shadow` layer: `[inset] <ox> <oy> [blur] [spread] <color>`. XP/98 bevels are all
// inset, blur 0, spread 0 — offsets alone carve the 1-2px light/dark edge bands.
#[derive(Clone)]
struct ShadowLayer {
    inset: bool,
    ox: f32,
    oy: f32,
    blur: f32,
    spread: f32,
    color: Color,
}

// A `linear-gradient(<dir>, <stop>, ...)`. Only axis-aligned (vertical/horizontal) directions;
// any off-axis angle snaps to the nearest axis (documented limit). Multi-stop = stacked quads.
#[derive(Clone)]
enum GradientDir {
    TopToBottom,
    BottomToTop,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone)]
struct GradientStop {
    color: Color,
    pos: Option<f32>, // 0..=1; `None` = auto (endpoints default 0/1, interior interpolated)
}

#[derive(Clone)]
struct Gradient {
    dir: GradientDir,
    stops: Vec<GradientStop>,
}

// Three interaction states baked per element. `hover`/`active` are the same cascade re-run with
// the matching pseudo-class asserted, so a `button:hover` rule (higher specificity) overlays the
// plain `button` rule in the hover slot. Lab 5 slice 3: `before`/`after` hold optional computed
// styles for `::before`/`::after` pseudo-elements matched against this element (NULL if no pseudo
// rule matches). Pseudo styles cascade independently of base/hover/active.
struct ElemStyles {
    base: ComputedStyle,
    hover: ComputedStyle,
    active: ComputedStyle,
    before: Option<ComputedStyle>,
    after: Option<ComputedStyle>,
}

struct StyleCtx {
    // None -> unstyled (Lab 1 path, byte-identical output). Some -> per-element computed styles
    // keyed by the element's stable AST pointer (the `body` tree is immutable for our lifetime).
    styles: Option<HashMap<*const Element, ElemStyles>>,
}

impl StyleCtx {
    fn get(&self, el: &Element) -> Option<&ElemStyles> {
        self.styles.as_ref()?.get(&(el as *const Element))
    }
}

// Flattened element for selector matching. `for`/`if` bodies are inlined (their children are
// descendants of the enclosing element and siblings of each other in document order).
struct NodeInfo {
    tag: String,
    class: Option<String>,
    parent: Option<usize>,
    prev: Option<usize>,
    ptr: *const Element,
}

fn compute_styles(css: &str, roots: &[BodyNode]) -> HashMap<*const Element, ElemStyles> {
    // Lab 5 pre-pass: extract :root vars and ::before/::after rules, then resolve var()/calc() in
    // every remaining declaration. simplecss sees only the cleaned text; pseudo rules (and the
    // resolved vars map) are kept on the StyleCtx for slice 3 to consume.
    let pre = preprocess_stylesheet(css);
    let sheet = simplecss::StyleSheet::parse(&pre.cleaned);

    // Apply rules low-specificity -> high so higher specificity overwrites; stable sort keeps
    // source order for ties (later-wins). simplecss specificity is [id, class, type].
    let mut order: Vec<usize> = (0..sheet.rules.len()).collect();
    order.sort_by_key(|&i| sheet.rules[i].selector.specificity());

    let mut arena: Vec<NodeInfo> = Vec::new();
    collect(roots, None, &mut None, &mut arena);

    let mut styles = HashMap::new();
    for idx in 0..arena.len() {
        let base = cascade(&sheet, &order, &arena, idx, false, false);
        let hover = cascade(&sheet, &order, &arena, idx, true, false);
        let active = cascade(&sheet, &order, &arena, idx, false, true);
        // Lab 5 slice 3: walk pseudo rules in source order, match each base against the element,
        // fold matching declarations into before/after slots. Pseudos don't vary by :hover/:active
        // (the demo doesn't exercise that; if needed, re-run with pseudo_class asserted).
        let mut before = ComputedStyle::default();
        let mut after = ComputedStyle::default();
        let mut has_before = false;
        let mut has_after = false;
        for p in &pre.pseudo {
            // Parse the base selector each call (cheap; simplecss parsing is fast). Skip on parse
            // failure (unknown pseudo-class in the base e.g. `:checked` would silently drop it,
            // matching how simplecss treats unknown pseudo-classes in the main cascade).
            if let Some(sel) = simplecss::Selector::parse(&p.base) {
                let node = MatchNode {
                    arena: &arena,
                    idx,
                    hover: false,
                    active: false,
                };
                if sel.matches(&node) {
                    let slot = match p.pseudo.as_str() {
                        "before" => {
                            has_before = true;
                            &mut before
                        }
                        "after" => {
                            has_after = true;
                            &mut after
                        }
                        _ => continue,
                    };
                    for (name, value) in &p.decls {
                        apply(slot, name, value);
                    }
                }
            }
        }
        styles.insert(
            arena[idx].ptr,
            ElemStyles {
                base,
                hover,
                active,
                before: has_before.then_some(before),
                after: has_after.then_some(after),
            },
        );
    }
    styles
}

fn collect(
    nodes: &[BodyNode],
    parent: Option<usize>,
    prev: &mut Option<usize>,
    arena: &mut Vec<NodeInfo>,
) {
    for node in nodes {
        match node {
            BodyNode::Element(el) => {
                let idx = arena.len();
                arena.push(NodeInfo {
                    tag: element_name(el),
                    class: class_of(el),
                    parent,
                    prev: *prev,
                    ptr: el as *const Element,
                });
                *prev = Some(idx);
                let mut child_prev = None;
                collect(&el.children, Some(idx), &mut child_prev, arena);
            }
            BodyNode::ForLoop(f) => collect(&f.body.roots, parent, prev, arena),
            BodyNode::IfChain(c) => collect_ifchain(c, parent, prev, arena),
            _ => {}
        }
    }
}

fn collect_ifchain(
    c: &IfChain,
    parent: Option<usize>,
    prev: &mut Option<usize>,
    arena: &mut Vec<NodeInfo>,
) {
    collect(&c.then_branch.roots, parent, prev, arena);
    if let Some(elif) = &c.else_if_branch {
        collect_ifchain(elif, parent, prev, arena);
    }
    if let Some(eb) = &c.else_branch {
        collect(&eb.roots, parent, prev, arena);
    }
}

fn cascade(
    sheet: &simplecss::StyleSheet,
    order: &[usize],
    arena: &[NodeInfo],
    idx: usize,
    hover: bool,
    active: bool,
) -> ComputedStyle {
    let node = MatchNode {
        arena,
        idx,
        hover,
        active,
    };
    let mut st = ComputedStyle::default();
    for &ri in order {
        let rule = &sheet.rules[ri];
        if rule.selector.matches(&node) {
            for d in &rule.declarations {
                apply(&mut st, d.name, d.value);
            }
        }
    }
    st
}

#[derive(Clone, Copy)]
struct MatchNode<'a> {
    arena: &'a [NodeInfo],
    idx: usize,
    hover: bool,
    active: bool,
}

impl simplecss::Element for MatchNode<'_> {
    fn parent_element(&self) -> Option<Self> {
        self.arena[self.idx]
            .parent
            .map(|p| MatchNode { idx: p, ..*self })
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.arena[self.idx]
            .prev
            .map(|p| MatchNode { idx: p, ..*self })
    }

    fn has_local_name(&self, name: &str) -> bool {
        self.arena[self.idx].tag == name
    }

    fn attribute_matches(&self, local_name: &str, operator: simplecss::AttributeOperator) -> bool {
        // simplecss lowers `.foo` to attribute_matches("class", Contains("foo")).
        if local_name != "class" {
            return false;
        }
        let Some(class) = &self.arena[self.idx].class else {
            return false;
        };
        use simplecss::AttributeOperator as Op;
        match operator {
            Op::Exists => true,
            Op::Matches(v) => class == v,
            Op::Contains(v) => class.split_whitespace().any(|c| c == v),
            Op::StartsWith(v) => class
                .split_whitespace()
                .any(|c| c == v || c.starts_with(&format!("{v}-"))),
        }
    }

    fn pseudo_class_matches(&self, class: simplecss::PseudoClass) -> bool {
        use simplecss::PseudoClass as Pc;
        match class {
            Pc::Hover => self.hover,
            Pc::Active => self.active,
            _ => false,
        }
    }
}

// Only the supported properties; unknown properties are ignored (documented subset).
fn apply(st: &mut ComputedStyle, name: &str, value: &str) {
    match name.trim() {
        "background-color" => {
            if let Some(c) = parse_color(value) {
                st.background_color = Some(c);
            }
        }
        // `background` shorthand: linear-gradient -> gradient (clears solid); else a solid color.
        "background" => {
            let v = value.trim();
            if v.starts_with("linear-gradient") {
                if let Some(g) = parse_linear_gradient(v) {
                    st.gradient = Some(g);
                    st.background_color = None;
                }
            } else if v == "none" {
                st.gradient = None;
                st.background_color = None;
            } else if let Some(c) = parse_color(v) {
                st.background_color = Some(c);
                st.gradient = None;
            }
        }
        // Lab 5 slice 2: svg-load -> rasterize at macro time, cache by path+scale, store the
        // Rc<Raster> on the computed style. `url("x.svg")` is treated identically (real CSS).
        "background-image" => {
            if let Some(inner) =
                extract_fn_call(value, "svg-load").or_else(|| extract_fn_call(value, "url"))
            {
                let parts = split_top_commas(inner);
                let path = parts.first().and_then(|p| parse_str_literal(p));
                let scale = parts
                    .get(1)
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(4); // default 4x so a 7x7 checkmark hits 28x28 (readable)
                if let Some(path) = path {
                    if let Ok(r) = rasterize_cached(&path, scale) {
                        st.background_image = Some(r);
                    }
                }
            }
        }
        "box-shadow" => {
            if let Some(layers) = parse_box_shadow(value) {
                st.box_shadow = Some(layers);
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                st.color = Some(c);
            }
        }
        "border" => {
            let (w, c) = parse_border(value);
            if w.is_some() {
                st.border_width = w;
            }
            if c.is_some() {
                st.border_color = c;
            }
        }
        "border-radius" => {
            if let Some(px) = parse_px(value) {
                st.border_radius = Some(px);
            }
        }
        "padding" => {
            if let Some(px) = parse_px(value.split_whitespace().next().unwrap_or(value)) {
                st.padding = Some(px);
            }
        }
        "font-size" => {
            if let Some(px) = parse_px(value) {
                st.font_size = Some(px);
            }
        }
        // Lab 4: flexbox layout. `display: flex` is the trigger to lower via egui_flex.
        "display" => {
            if value.trim() == "flex" {
                st.display_flex = true;
            }
        }
        "flex-direction" => {
            st.flex_column = matches!(value.trim(), "column" | "column-reverse");
        }
        "flex-flow" | "flex-wrap" => {
            // Parsed but unused: wrapping/multi-line flex is out of Lab 4 scope (single line only).
        }
        "gap" => {
            // `gap: <row-gap> [col-gap]`; we apply one value to both axes. Multi-value collapses.
            if let Some(px) = parse_px(value.split_whitespace().next().unwrap_or(value)) {
                st.gap = Some(px);
            }
        }
        "width" => {
            if let Some(l) = parse_len(value) {
                st.width = Some(l);
            }
        }
        "height" => {
            if let Some(l) = parse_len(value) {
                st.height = Some(l);
            }
        }
        "flex-grow" => {
            if let Ok(n) = value.trim().parse::<f32>() {
                st.flex_grow = Some(n);
            }
        }
        "flex" => {
            // `flex: <grow> <shrink> <basis>` shorthand. Only `grow` is consumed (Lab 4 subset).
            if let Some(first) = value.split_whitespace().next() {
                if let Ok(n) = first.parse::<f32>() {
                    st.flex_grow = Some(n);
                }
            }
        }
        "justify-content" => {
            st.justify = match value.trim() {
                "flex-start" | "start" => Some(FlexJustify::Start),
                "flex-end" | "end" => Some(FlexJustify::End),
                "center" => Some(FlexJustify::Center),
                "space-between" => Some(FlexJustify::SpaceBetween),
                "space-around" => Some(FlexJustify::SpaceAround),
                "space-evenly" => Some(FlexJustify::SpaceEvenly),
                _ => None,
            };
        }
        "align-items" => {
            st.align = match value.trim() {
                "flex-start" | "start" => Some(FlexAlign::Start),
                "flex-end" | "end" => Some(FlexAlign::End),
                "center" => Some(FlexAlign::Center),
                "stretch" => Some(FlexAlign::Stretch),
                _ => None,
            };
        }
        _ => {}
    }
}

// Lab 4: `<px>` -> Len::Points, `<%>` -> Len::Percent (0.0..=1.0). `auto`/anything else -> None.
fn parse_len(s: &str) -> Option<Len> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix('%') {
        let p = num.trim().parse::<f32>().ok()?;
        return Some(Len::Percent(p / 100.0));
    }
    parse_px(s).map(Len::Points)
}

// `#rgb`, `#rrggbb`, `#rrggbbaa`, or one of the named colors xp.css uses. Alpha defaults to 255.
fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(c) = named_color(s) {
        return Some(c);
    }
    let h = s.strip_prefix('#')?;
    let byte = |x: &str| u8::from_str_radix(x, 16).ok();
    match h.len() {
        3 => {
            let r = byte(&h[0..1])?;
            let g = byte(&h[1..2])?;
            let b = byte(&h[2..3])?;
            Some((r * 17, g * 17, b * 17, 255))
        }
        6 => Some((byte(&h[0..2])?, byte(&h[2..4])?, byte(&h[4..6])?, 255)),
        8 => Some((
            byte(&h[0..2])?,
            byte(&h[2..4])?,
            byte(&h[4..6])?,
            byte(&h[6..8])?,
        )),
        _ => None,
    }
}

// The subset of CSS named colors xp.css/98.css reference. Everything else must be hex.
fn named_color(s: &str) -> Option<Color> {
    Some(match s {
        "white" => (255, 255, 255, 255),
        "black" => (0, 0, 0, 255),
        "grey" | "gray" => (128, 128, 128, 255),
        "silver" => (192, 192, 192, 255),
        "lightblue" => (173, 216, 230, 255),
        "transparent" => (0, 0, 0, 0),
        _ => return None,
    })
}

fn parse_px(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").trim().parse().ok()
}

// `1px solid #0054e3` -> (width, color); style keyword ignored (only `solid` supported implicitly).
fn parse_border(s: &str) -> (Option<f32>, Option<Color>) {
    let mut width = None;
    let mut color = None;
    for tok in s.split_whitespace() {
        if let Some(px) = parse_px(tok) {
            width = Some(px);
        } else if let Some(c) = parse_color(tok) {
            color = Some(c);
        }
    }
    (width, color)
}

// Split on top-level commas only (parens protect nested lists like `rgb(a,b,c)` or gradient
// stops). Used by both the box-shadow layer list and the gradient stop list.
fn split_top_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

// `[inset] <ox> <oy> [blur] [spread] <color>` per comma-separated layer. `none` -> empty stack.
// blur/spread are captured but only enforced at lowering time (blur > 0 is unsupported -> inert).
fn parse_box_shadow(value: &str) -> Option<Vec<ShadowLayer>> {
    let v = value.trim();
    if v == "none" {
        return Some(vec![]);
    }
    let mut layers = Vec::new();
    for part in split_top_commas(v) {
        let mut inset = false;
        let mut nums: Vec<f32> = Vec::new();
        let mut color: Option<Color> = None;
        for tok in part.split_whitespace() {
            if tok == "inset" {
                inset = true;
            } else if let Some(px) = parse_px(tok) {
                nums.push(px);
            } else if let Some(c) = parse_color(tok) {
                color = Some(c);
            }
        }
        if nums.len() < 2 {
            continue; // need at least ox, oy
        }
        layers.push(ShadowLayer {
            inset,
            ox: nums[0],
            oy: nums[1],
            blur: nums.get(2).copied().unwrap_or(0.0),
            spread: nums.get(3).copied().unwrap_or(0.0),
            color: color.unwrap_or((0, 0, 0, 255)),
        });
    }
    Some(layers)
}

fn parse_linear_gradient(v: &str) -> Option<Gradient> {
    let inner = v
        .trim()
        .strip_prefix("linear-gradient")?
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let parts = split_top_commas(inner);
    if parts.len() < 2 {
        return None;
    }
    // First part is a direction only if it is not itself a color stop.
    let (dir, stop_parts): (GradientDir, &[String]) = match parse_gradient_dir(&parts[0]) {
        Some(d) => (d, &parts[1..]),
        None => (GradientDir::TopToBottom, &parts[..]),
    };
    let mut stops = Vec::new();
    for p in stop_parts {
        let toks: Vec<&str> = p.split_whitespace().collect();
        let color = parse_color(toks.first()?)?;
        let pos = toks
            .get(1)
            .and_then(|t| t.strip_suffix('%'))
            .and_then(|n| n.trim().parse::<f32>().ok())
            .map(|x| x / 100.0);
        stops.push(GradientStop { color, pos });
    }
    if stops.len() < 2 {
        return None;
    }
    Some(Gradient { dir, stops })
}

fn parse_gradient_dir(s: &str) -> Option<GradientDir> {
    let s = s.trim();
    match s {
        "to bottom" | "180deg" => return Some(GradientDir::TopToBottom),
        "to top" | "0deg" => return Some(GradientDir::BottomToTop),
        "to right" | "90deg" => return Some(GradientDir::LeftToRight),
        "to left" | "270deg" => return Some(GradientDir::RightToLeft),
        _ => {}
    }
    // Any other explicit angle: snap to the nearest axis (vertical/horizontal support only).
    let a = s.strip_suffix("deg")?.trim().parse::<f32>().ok()?;
    let a = ((a % 360.0) + 360.0) % 360.0;
    Some(if a < 45.0 || a >= 315.0 {
        GradientDir::BottomToTop
    } else if a < 135.0 {
        GradientDir::LeftToRight
    } else if a < 225.0 {
        GradientDir::TopToBottom
    } else {
        GradientDir::RightToLeft
    })
}

// Resolve every stop's position (fill auto endpoints + interior gaps) and clamp to non-decreasing,
// then re-express as a fraction along the VISUAL axis (top->bottom / left->right) so the mesh
// builder is direction-agnostic. Returns points sorted by visual fraction.
fn gradient_points(g: &Gradient) -> Vec<(f32, Color)> {
    let n = g.stops.len();
    let mut pos: Vec<Option<f32>> = g.stops.iter().map(|s| s.pos).collect();
    if pos[0].is_none() {
        pos[0] = Some(0.0);
    }
    if pos[n - 1].is_none() {
        pos[n - 1] = Some(1.0);
    }
    let mut i = 0;
    while i < n {
        if pos[i].is_some() {
            i += 1;
            continue;
        }
        let start = i - 1;
        let sv = pos[start].unwrap();
        let mut j = i;
        while pos[j].is_none() {
            j += 1;
        }
        let ev = pos[j].unwrap();
        let gap = (j - start) as f32;
        for k in i..j {
            pos[k] = Some(sv + (ev - sv) * ((k - start) as f32) / gap);
        }
        i = j;
    }
    let reversed = matches!(g.dir, GradientDir::BottomToTop | GradientDir::RightToLeft);
    let mut maxp = 0.0f32;
    let mut pts: Vec<(f32, Color)> = g
        .stops
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let mut p = pos[k].unwrap();
            if p < maxp {
                p = maxp;
            } else {
                maxp = p;
            }
            let vf = if reversed { 1.0 - p } else { p };
            (vf, s.color)
        })
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    pts
}

// --- Lab 5 slice 2: SVG rasterization for `background-image: svg-load(...)` ----

// Rasterized SVG bytes + dims. The macro rasterizes once (cached by path+scale) and bakes the
// bytes as a `&[u8]` literal; the runtime constructs a ColorImage and uploads once via the egui
// TextureManager (cached per-call-site via an `OnceLock<TextureHandle>` static in the expansion).
// resvg/usvg/tiny-skia live ONLY in the proc-macro; the wasm/native target never links them.
#[derive(Debug)]
struct Raster {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

thread_local! {
    // Process-wide cache so a stylesheet that references the same SVG in N rules rasterizes once.
    // Keyed by (path, scale); path resolves against CARGO_MANIFEST_DIR of the CALLER crate.
    static SVG_CACHE: RefCell<HashMap<(String, u32), Rc<Raster>>> = RefCell::new(HashMap::new());
}

fn rasterize_cached(path: &str, scale: u32) -> Result<Rc<Raster>, String> {
    SVG_CACHE.with(|c| {
        if let Some(r) = c.borrow().get(&(path.to_string(), scale)) {
            return Ok(r.clone());
        }
        let r = Rc::new(rasterize_svg(path, scale)?);
        c.borrow_mut().insert((path.to_string(), scale), r.clone());
        Ok(r)
    })
}

// Parse + rasterize. Path resolves against CARGO_MANIFEST_DIR of the caller crate (same as the
// stylesheet header). Scale multiplies the SVG's native pixel dims. Default Options are fine for
// path-only SVGs (no fontdb load); text-bearing SVGs would need load_system_fonts.
fn rasterize_svg(path: &str, scale: u32) -> Result<Raster, String> {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").map_err(|_| "CARGO_MANIFEST_DIR unset".to_string())?;
    let full = std::path::Path::new(&manifest).join(path);
    let data =
        std::fs::read(&full).map_err(|e| format!("cannot read svg `{}`: {e}", full.display()))?;

    let opt = usvg::Options {
        resources_dir: full.parent().map(|p| p.to_path_buf()),
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_data(&data, &opt).map_err(|e| format!("usvg parse `{}`: {e:?}", path))?;

    let native = tree.size().to_int_size();
    let w = native
        .width()
        .checked_mul(scale)
        .ok_or_else(|| format!("svg `{path}`: width overflow at scale {scale}"))?;
    let h = native
        .height()
        .checked_mul(scale)
        .ok_or_else(|| format!("svg `{path}`: height overflow at scale {scale}"))?;
    let mut pixmap =
        tiny_skia::Pixmap::new(w, h).ok_or_else(|| format!("pixmap {w}x{h} alloc failed"))?;
    let transform = if scale <= 1 {
        tiny_skia::Transform::default()
    } else {
        tiny_skia::Transform::from_scale(scale as f32, scale as f32)
    };
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok(Raster {
        bytes: pixmap.data().to_vec(),
        width: w,
        height: h,
    })
}

// Pull the inner of a CSS function call: `svg-load("x.svg", 4)` -> Some(`"x.svg", 4`). Matches the
// FIRST `name(` and balance-matches the close. None if the prefix isn't `name(`.
fn extract_fn_call<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let v = value.trim();
    let prefix = format!("{name}(");
    let start = v.strip_prefix(&prefix)?.as_ptr() as usize;
    let v_start = v.as_ptr() as usize;
    let offset = start - v_start;
    // balance-match the close paren
    let bytes = v.as_bytes();
    let mut depth = 1i32;
    let mut i = offset;
    while i < bytes.len() && depth > 0 {
        if bytes[i] == b'(' {
            depth += 1;
        } else if bytes[i] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(v[offset..i].trim());
            }
        }
        i += 1;
    }
    None
}

// Strip a single layer of `"..."` or `'...'` quoting. None if not a string literal.
fn parse_str_literal(s: &str) -> Option<String> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let (open, close) = (bytes[0], bytes[bytes.len() - 1]);
    if (open == b'"' && close == b'"') || (open == b'\'' && close == b'\'') {
        Some(s[1..s.len() - 1].to_string())
    } else {
        None
    }
}

// --- Lab 5: var() + calc() + ::before/::after pre-pass ---------------------

// CSS custom properties (`--foo: bar`) and `calc()` resolve at MACRO time. The expansion still
// bakes concrete literals. xp.css writes bevels as `var(--border-raised-outer),
// var(--border-raised-inner)` and the radio/checkbox spacing as `calc(var(--radio-total-width-
// precalc))`; both flatten here so simplecss and the property parsers see only concrete values.
//
// simplecss 0.2.2 has no `:root` pseudo-class and no `::before`/`::after` pseudo-element support
// (selector.rs:317-320: unknown pseudo-classes log a warning and the rule is dropped). So a text
// pre-pass extracts both before handing the rest to simplecss. Pseudo-element rules are stored
// here for slice 3 to consume; var+calc land in slice 1.

type Vars = HashMap<String, String>;

// A pseudo-element rule peeled off in the pre-pass. `base` is the selector with the trailing
// `::name` removed; `pseudo` is "before" or "after"; `decls` are the rule's declarations.
#[allow(dead_code)] // consumed by slice 3 (pseudo-element emission)
struct PseudoRule {
    base: String,
    pseudo: String,
    decls: Vec<(String, String)>,
}

#[allow(dead_code)] // `vars` consumed by slice 2 (svg-load path lookups); `pseudo` by slice 3
struct Preprocessed {
    vars: Vars,
    pseudo: Vec<PseudoRule>,
    // The stylesheet text with :root blocks and pseudo-element rules removed, and every remaining
    // declaration value resolved (var substituted, calc evaluated). This is what simplecss parses.
    cleaned: String,
}

// Top-level pre-pass. Tokenize -> classify -> resolve -> emit cleaned text.
fn preprocess_stylesheet(css: &str) -> Preprocessed {
    let rules = scan_rule_blocks(css);
    let mut vars: Vars = Vars::new();
    let mut pseudo: Vec<PseudoRule> = Vec::new();
    let mut kept: Vec<(String, Vec<(String, String)>)> = Vec::new();

    for (sel_text, body_text) in &rules {
        let decls = parse_decls(body_text);
        let trimmed = sel_text.trim();
        // `:root` selector: peel `--x: y;` into Vars, drop the block. simplecss would drop it
        // anyway (no `:root` pseudo-class); this rescues the custom properties.
        if trimmed == ":root"
            || trimmed.starts_with(":root,")
            || trimmed.split(',').any(|p| p.trim() == ":root")
        {
            for (k, v) in decls {
                if k.trim().starts_with("--") {
                    vars.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            continue;
        }
        // `::before`/`::after` rule: split each comma-group member; members with a pseudo-element
        // become PseudoRules, members without stay as a kept rule (xp.css never mixes these).
        // Decls are stored RAW here; var()/calc() resolution happens in a second pass below,
        // once the :root vars fixpoint has been computed (vars can reference each other).
        if let Some(split) = split_pseudo(trimmed) {
            for (base, pseudo_name) in split {
                pseudo.push(PseudoRule {
                    base,
                    pseudo: pseudo_name,
                    decls: decls.clone(),
                });
            }
            continue;
        }
        kept.push((trimmed.to_string(), decls));
    }

    // Custom props can reference other props (--x: var(--y)); resolve the fixpoint before using.
    let resolved_vars = resolve_vars_fixpoint(&vars);

    // Second pass over pseudo rules: resolve var()/calc() in every declaration now that the vars
    // fixpoint has settled. The cascade-time apply() then sees only concrete literals, matching
    // the invariant for kept rules (which were resolved inline into the cleaned text above).
    for p in pseudo.iter_mut() {
        for (_k, v) in p.decls.iter_mut() {
            *v = resolve_value(v, &resolved_vars);
        }
    }

    let mut cleaned = String::with_capacity(css.len());
    for (sel, decls) in &kept {
        cleaned.push_str(sel);
        cleaned.push_str(" { ");
        for (k, v) in decls {
            let resolved = resolve_value(v, &resolved_vars);
            cleaned.push_str(k);
            cleaned.push_str(": ");
            cleaned.push_str(&resolved);
            cleaned.push_str("; ");
        }
        cleaned.push_str("}\n");
    }

    Preprocessed {
        vars: resolved_vars,
        pseudo,
        cleaned,
    }
}

// Tokenize a stylesheet into (selector_text, body_text) pairs. Brace-aware (nested braces in
// declarations are tracked); comment-aware. At-rules (`@media`, `@keyframes`) are skipped with
// their outermost brace block so they don't pollute the rule list. ASCII-only assumption (xp.css
// and the demo CSS are ASCII).
fn scan_rule_blocks(css: &str) -> Vec<(String, String)> {
    let mut rules = Vec::new();
    let bytes = css.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut cur_sel = String::new();
    while i < n {
        // skip comments
        if bytes[i..].starts_with(b"/*") {
            match css[i + 2..].find("*/") {
                Some(end) => {
                    i += 2 + end + 2;
                    continue;
                }
                None => break,
            }
        }
        if bytes[i] == b'@' {
            // at-rule: skip selector + its block (or single statement if no block)
            cur_sel.clear();
            // advance past selector up to `{` or `;`
            while i < n && bytes[i] != b'{' && bytes[i] != b';' {
                if bytes[i..].starts_with(b"/*") {
                    match css[i + 2..].find("*/") {
                        Some(end) => {
                            i += 2 + end + 2;
                            continue;
                        }
                        None => {
                            i = n;
                            break;
                        }
                    }
                }
                i += 1;
            }
            if i < n && bytes[i] == b'{' {
                // skip the brace-balanced block
                let mut depth: i32 = 1;
                i += 1;
                while i < n && depth > 0 {
                    if bytes[i..].starts_with(b"/*") {
                        match css[i + 2..].find("*/") {
                            Some(end) => {
                                i += 2 + end + 2;
                                continue;
                            }
                            None => {
                                i = n;
                                break;
                            }
                        }
                    }
                    if bytes[i] == b'{' {
                        depth += 1;
                    } else if bytes[i] == b'}' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                if i < n {
                    i += 1;
                }
            } else if i < n && bytes[i] == b';' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'{' {
            let body_start = i + 1;
            let mut depth: i32 = 1;
            i += 1;
            while i < n && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    match css[i + 2..].find("*/") {
                        Some(end) => {
                            i += 2 + end + 2;
                            continue;
                        }
                        None => {
                            i = n;
                            break;
                        }
                    }
                }
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                }
                if depth > 0 {
                    i += 1;
                }
            }
            let body = css[body_start..i.min(n)].trim().to_string();
            rules.push((std::mem::take(&mut cur_sel), body));
            if i < n {
                i += 1;
            }
        } else {
            cur_sel.push(bytes[i] as char);
            i += 1;
        }
    }
    rules
}

// Parse `--x: y; foo: bar;` into owned (name, value) pairs. Strips comments first; splits on `;`
// then on the first `:`. Empty entries dropped. No value parsing here — values stay as strings so
// the caller can resolve var/calc before re-emitting.
fn parse_decls(body: &str) -> Vec<(String, String)> {
    // strip comments
    let mut clean = String::with_capacity(body.len());
    let mut s = body;
    while let Some(start) = s.find("/*") {
        clean.push_str(&s[..start]);
        let rest = &s[start + 2..];
        match rest.find("*/") {
            Some(end) => {
                s = &rest[end + 2..];
            }
            None => {
                s = "";
            }
        }
    }
    clean.push_str(s);

    let mut out = Vec::new();
    for decl in clean.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        if let Some((k, v)) = decl.split_once(':') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

// Split a selector like `input[type="checkbox"]:checked + label::after` into a list of
// (base, pseudo_name) pairs, one per comma-group member that has a pseudo-element. Members
// without a pseudo-element are dropped (xp.css never mixes pseudo + non-pseudo in one selector;
// if it did, we'd lose the non-pseudo half — documented limit). Handles `::before`/`::after`
// (CSS3) and `:before`/`:after` (CSS2). Returns None if no member has a pseudo-element.
fn split_pseudo(sel: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for member in sel.split(',') {
        let m = member.trim();
        // rfind for the rightmost pseudo so `:hover::before` splits at `::before`, not `:hover`.
        // The pseudo must be terminal (no following combinator) — verified by checking the suffix
        // after the match is empty.
        let mut matched = false;
        for pseudo in ["before", "after"] {
            let needle = format!("::{pseudo}");
            if let Some(idx) = m.rfind(&needle) {
                if m[idx + needle.len()..].trim().is_empty() {
                    let base = m[..idx].trim_end().to_string();
                    if !base.is_empty() {
                        out.push((base, pseudo.to_string()));
                        matched = true;
                    }
                    break;
                }
            }
            let needle = format!(":{pseudo}");
            if let Some(idx) = m.rfind(&needle) {
                if m[idx + needle.len()..].trim().is_empty() {
                    let base = m[..idx].trim_end().to_string();
                    if !base.is_empty() {
                        out.push((base, pseudo.to_string()));
                        matched = true;
                    }
                    break;
                }
            }
        }
        let _ = matched;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// Iterate var() resolution over the Vars map until it stabilizes or a depth limit fires (cheap
// cycle guard). xp.css nests at most 2 levels (--radio-total-width -> precalc -> raw).
fn resolve_vars_fixpoint(vars: &Vars) -> Vars {
    let mut out = vars.clone();
    for _ in 0..8 {
        let mut changed = false;
        let keys: Vec<String> = out.keys().cloned().collect();
        for k in keys {
            let v = out[&k].clone();
            let resolved = substitute_vars(&v, &out);
            if resolved != v {
                out.insert(k, resolved);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    out
}

// Replace `var(--x)` and `var(--x, fallback)` with the resolved value. Unresolved vars with no
// fallback are left as `var(--x)` so the property parser sees the failure mode and rejects it.
// Fallbacks are themselves recursively substituted (so chained `var(--x, var(--y, 6px))` works).
fn substitute_vars(s: &str, vars: &Vars) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        if bytes[i..].starts_with(b"var(") {
            let start = i + 4;
            let mut depth: i32 = 1;
            let mut j = start;
            while j < n && depth > 0 {
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                if depth > 0 {
                    j += 1;
                }
            }
            let inner = &s[start..j.min(n)];
            i = if j < n { j + 1 } else { j };
            let parts = split_top_commas(inner);
            let (name, fallback): (String, Option<String>) = match parts.as_slice() {
                [] => (String::new(), None),
                [only] => (only.clone(), None),
                [first, rest @ ..] => (first.clone(), Some(rest.join(","))),
            };
            let name = name.trim();
            if let Some(v) = vars.get(name) {
                out.push_str(v);
            } else if let Some(fb) = fallback {
                out.push_str(&substitute_vars(fb.trim(), vars));
            } else {
                // unresolved and no fallback: leave the literal so the value is visibly bad
                out.push_str("var(");
                out.push_str(inner);
                out.push(')');
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// Resolve a single value: substitute vars, then evaluate each top-level calc(...) to a concrete
// number+unit. calc-inside-calc is handled because substitute_vars runs first (a calc containing
// a var resolves the var, then we evaluate the calc).
fn resolve_value(s: &str, vars: &Vars) -> String {
    let with_vars = substitute_vars(s, vars);
    let bytes = with_vars.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(with_vars.len());
    let mut i = 0;
    while i < n {
        if bytes[i..].starts_with(b"calc(") {
            let start = i + 5;
            let mut depth: i32 = 1;
            let mut j = start;
            while j < n && depth > 0 {
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                if depth > 0 {
                    j += 1;
                }
            }
            let inner = &with_vars[start..j.min(n)];
            i = if j < n { j + 1 } else { j };
            if let Some((num, unit)) = eval_calc_expr(inner) {
                out.push_str(&format_number(num));
                if !unit.is_empty() {
                    out.push_str(&unit);
                }
            } else {
                out.push_str("calc(");
                out.push_str(inner);
                out.push(')');
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// calc() evaluator. Grammar:
//   expr   := term (('+' | '-') term)*
//   term   := factor (('*' | '/') factor)*
//   factor := ['-' | '+'] number [unit] | '(' expr ')'
// Unit handling: the first non-empty unit seen on either operand wins (xp.css calc operands are
// uniformly px). Mixed-unit arithmetic is the caller's problem; we don't error.
#[derive(Debug, Clone)]
enum CalcTok {
    Num(f32, String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn eval_calc_expr(s: &str) -> Option<(f32, String)> {
    // Flatten nested calc(): CSS `calc(calc(x) op y)` == `calc((x) op y)`. Replace the inner
    // `calc(` with `(` so the existing tokenizer + paren-aware parser handles nesting for free.
    let flattened = s.replace("calc(", "(");
    let tokens = tokenize_calc(&flattened);
    let mut pos = 0;
    let result = parse_calc_expr(&tokens, &mut pos)?;
    // leftover tokens = unbalanced -> the input was malformed; fail.
    if pos != tokens.len() {
        return None;
    }
    Some(result)
}

fn tokenize_calc(s: &str) -> Vec<CalcTok> {
    let mut toks = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
            }
            b'+' => {
                toks.push(CalcTok::Plus);
                i += 1;
            }
            b'*' => {
                toks.push(CalcTok::Star);
                i += 1;
            }
            b'/' => {
                toks.push(CalcTok::Slash);
                i += 1;
            }
            b'(' => {
                toks.push(CalcTok::LParen);
                i += 1;
            }
            b')' => {
                toks.push(CalcTok::RParen);
                i += 1;
            }
            b'-' => {
                // Distinguish a leading sign on a number from a binary minus. After a Num or `)`,
                // `-` is binary; otherwise it's a sign consumed with the following number.
                let is_binary = matches!(
                    toks.last(),
                    Some(CalcTok::Num(_, _)) | Some(CalcTok::RParen)
                );
                if is_binary {
                    toks.push(CalcTok::Minus);
                    i += 1;
                } else {
                    let (n, unit, next) = read_number(&s[i..]);
                    toks.push(CalcTok::Num(n, unit));
                    i += next;
                }
            }
            b'0'..=b'9' | b'.' => {
                let (n, unit, next) = read_number(&s[i..]);
                toks.push(CalcTok::Num(n, unit));
                i += next;
            }
            _ => break, // unknown char: stop tokenizing; eval_calc_expr will fail on partial parse
        }
    }
    toks
}

fn read_number(s: &str) -> (f32, String, usize) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut sign = 1.0f32;
    if bytes.first() == Some(&b'-') {
        sign = -1.0;
        i += 1;
    } else if bytes.first() == Some(&b'+') {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    let n: f32 = sign * s[start..i].parse().unwrap_or(0.0);
    let unit_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'%') {
        i += 1;
    }
    let unit = s[unit_start..i].to_string();
    (n, unit, i)
}

fn parse_calc_expr(toks: &[CalcTok], pos: &mut usize) -> Option<(f32, String)> {
    let (mut val, mut unit) = parse_calc_term(toks, pos)?;
    while *pos < toks.len() {
        match &toks[*pos] {
            CalcTok::Plus => {
                *pos += 1;
                let (rhs, u2) = parse_calc_term(toks, pos)?;
                if unit.is_empty() {
                    unit = u2;
                }
                val += rhs;
            }
            CalcTok::Minus => {
                *pos += 1;
                let (rhs, u2) = parse_calc_term(toks, pos)?;
                if unit.is_empty() {
                    unit = u2;
                }
                val -= rhs;
            }
            _ => break,
        }
    }
    Some((val, unit))
}

fn parse_calc_term(toks: &[CalcTok], pos: &mut usize) -> Option<(f32, String)> {
    let (mut val, mut unit) = parse_calc_factor(toks, pos)?;
    while *pos < toks.len() {
        match &toks[*pos] {
            CalcTok::Star => {
                *pos += 1;
                let (rhs, u2) = parse_calc_factor(toks, pos)?;
                if unit.is_empty() {
                    unit = u2;
                }
                val *= rhs;
            }
            CalcTok::Slash => {
                *pos += 1;
                let (rhs, _) = parse_calc_factor(toks, pos)?;
                // CSS calc: division by zero makes the whole declaration invalid. Fail rather
                // than silently produce Inf/NaN/0.
                if rhs == 0.0 {
                    return None;
                }
                val /= rhs;
            }
            _ => break,
        }
    }
    Some((val, unit))
}

fn parse_calc_factor(toks: &[CalcTok], pos: &mut usize) -> Option<(f32, String)> {
    if *pos >= toks.len() {
        return None;
    }
    match &toks[*pos] {
        CalcTok::Num(n, u) => {
            let v = (*n, u.clone());
            *pos += 1;
            Some(v)
        }
        CalcTok::LParen => {
            *pos += 1;
            let v = parse_calc_expr(toks, pos)?;
            // Require the matching `)` — an unbalanced paren must fail the whole eval, otherwise
            // `(2 + 3` would parse as a valid 5 with a leftover `(` the top-level check doesn't
            // catch (pos hits end simultaneously with skipping the close).
            if matches!(toks.get(*pos), Some(CalcTok::RParen)) {
                *pos += 1;
            } else {
                return None;
            }
            Some(v)
        }
        _ => None,
    }
}

fn format_number(n: f32) -> String {
    // Trim trailing zeros so expanded.rs reads cleanly: 4.0 -> "4", 4.5 -> "4.5". Negative zero
    // normalizes to "0" so two paths to the same value produce identical baked literals.
    let n = if n == 0.0 { 0.0 } else { n };
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// --- shared helpers --------------------------------------------------------

fn element_name(el: &Element) -> String {
    match &el.name {
        ElementName::Ident(i) => i.to_string(),
        ElementName::Custom(s) => s.value(),
    }
}

// `class: "panel"` -> the static string. dioxus wraps it as AttrLiteral(HotLiteral::Fmted(..));
// IfmtInput::to_static returns Some only when every segment is a literal (no `{expr}` splice).
fn class_of(el: &Element) -> Option<String> {
    el.raw_attributes
        .iter()
        .find_map(|attr| match (&attr.name, &attr.value) {
            (AttributeName::BuiltIn(id), AttributeValue::AttrLiteral(HotLiteral::Fmted(seg)))
                if *id == "class" =>
            {
                seg.formatted_input.to_static()
            }
            _ => None,
        })
}

// rsx text is an IfmtInput; its own ToTokens yields a static `&str` or `::std::format!(..)` —
// exactly what ui.label/RichText::new want.
fn text_tokens(text: &TextNode) -> TokenStream2 {
    text.input.formatted_input.to_token_stream()
}

fn text_children(el: &Element) -> Vec<&TextNode> {
    el.children
        .iter()
        .filter_map(|c| match c {
            BodyNode::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn first_text(el: &Element) -> Option<&TextNode> {
    text_children(el).into_iter().next()
}

fn onclick(el: &Element) -> Option<&PartialClosure> {
    el.raw_attributes
        .iter()
        .find_map(|attr| match (&attr.name, &attr.value) {
            (AttributeName::BuiltIn(id), AttributeValue::EventTokens(closure))
                if *id == "onclick" =>
            {
                Some(closure)
            }
            _ => None,
        })
}

// `class:` value survives into the expansion as an inert underscore binding (token streams carry
// no `//` comments). It documents which class drove the styling; the computed values are baked
// into the surrounding egui calls, not read from this binding at runtime.
fn class_markers(el: &Element) -> TokenStream2 {
    let mut out = TokenStream2::new();
    for attr in &el.raw_attributes {
        if let (AttributeName::BuiltIn(id), AttributeValue::AttrLiteral(lit)) =
            (&attr.name, &attr.value)
        {
            if *id == "class" {
                out.extend(quote! { let _egui_rsx_class = #lit; });
            }
        }
    }
    out
}

// --- Lab 5 tests (no-wgpu, fast) ------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_basic_lookup() {
        let mut v = Vars::new();
        v.insert("--face".to_string(), "#dfdfdf".to_string());
        assert_eq!(substitute_vars("var(--face)", &v), "#dfdfdf");
        assert_eq!(substitute_vars("a var(--face) b", &v), "a #dfdfdf b");
    }

    #[test]
    fn var_fallback_when_unresolved() {
        let v = Vars::new();
        // unresolved with fallback uses the fallback
        assert_eq!(substitute_vars("var(--missing, 6px)", &v), "6px");
        // unresolved with no fallback leaves the literal so the failure is visible downstream
        assert_eq!(substitute_vars("var(--missing)", &v), "var(--missing)");
    }

    #[test]
    fn var_chain_resolves_through_fixpoint() {
        // mirrors xp.css's --radio-total-width: calc(var(--radio-total-width-precalc)) where
        // precalc = width + label-spacing
        let mut v = Vars::new();
        v.insert("--radio-width".into(), "12px".into());
        v.insert("--radio-label-spacing".into(), "6px".into());
        v.insert(
            "--radio-total-width-precalc".into(),
            "var(--radio-width) + var(--radio-label-spacing)".into(),
        );
        v.insert(
            "--radio-total-width".into(),
            "calc(var(--radio-total-width-precalc))".into(),
        );
        let resolved = resolve_vars_fixpoint(&v);
        // The fixpoint must chase the chain: precalc resolves to "12px + 6px" (string concat at
        // this stage), then --radio-total-width resolves to calc(12px + 6px). resolve_value then
        // evaluates the calc to 18px.
        assert_eq!(
            resolved.get("--radio-total-width").unwrap(),
            "calc(12px + 6px)"
        );
        assert_eq!(resolve_value("var(--radio-total-width)", &resolved), "18px");
    }

    #[test]
    fn calc_precedence_and_signs() {
        // xp.css uses `-1 * var(--x)` and `var(--a) / 2 - var(--b) / 2`. Verify precedence.
        assert_eq!(eval_calc_expr("2 + 3 * 4").unwrap().0, 14.0); // 2 + (3*4)
        assert_eq!(eval_calc_expr("2 * 3 + 4").unwrap().0, 10.0); // (2*3) + 4
        assert_eq!(eval_calc_expr("(2 + 3) * 4").unwrap().0, 20.0); // parens override
        assert_eq!(eval_calc_expr("-1 * 12").unwrap().0, -12.0); // sign prefix
        assert_eq!(eval_calc_expr("12 / 2").unwrap().0, 6.0);
        assert_eq!(eval_calc_expr("12 / 2 - 4 / 2").unwrap().0, 4.0); // 6 - 2
        assert_eq!(eval_calc_expr("-1 * (12 + 6)").unwrap().0, -18.0); // sign + parens
    }

    #[test]
    fn calc_unit_carry() {
        // first non-empty unit wins; xp.css calc operands are uniformly px
        let (n, u) = eval_calc_expr("1px * -1").unwrap();
        assert_eq!(n, -1.0);
        assert_eq!(u, "px");
        let (n, u) = eval_calc_expr("12px + 6px").unwrap();
        assert_eq!(n, 18.0);
        assert_eq!(u, "px");
        // unitless result stays unitless
        let (n, u) = eval_calc_expr("2 + 2").unwrap();
        assert_eq!(n, 4.0);
        assert_eq!(u, "");
    }

    #[test]
    fn calc_division_by_zero_fails() {
        // CSS calc: a /0 makes the declaration invalid. Returns None so the value resolver leaves
        // the literal calc() in place (visible in expanded.rs as a deliberate-failure marker).
        assert!(eval_calc_expr("12 / 0").is_none());
        assert!(eval_calc_expr("12 / (2 - 2)").is_none());
    }

    #[test]
    fn calc_malformed_returns_none() {
        // unbalanced parens, leftover tokens, unknown chars all fail
        assert!(eval_calc_expr("(2 + 3").is_none());
        assert!(eval_calc_expr("2 +").is_none());
        assert!(eval_calc_expr("2 + + 3").is_none()); // dangling operator
        assert!(eval_calc_expr("foo").is_none());
    }

    #[test]
    fn resolve_value_handles_var_inside_calc() {
        let mut v = Vars::new();
        v.insert("--bevel".into(), "1px".into());
        v.insert("--w".into(), "12px".into());
        // the exact shape from xp.css's button bevel rewrite: calc(var(--bevel) * -1)
        assert_eq!(resolve_value("calc(var(--bevel) * -1)", &v), "-1px");
        // nested calc inside calc (after var substitution): outer calc evaluates the whole thing
        assert_eq!(resolve_value("calc(calc(12px + 6px) / 2)", &v), "9px");
        // var outside calc stays a literal substitution
        assert_eq!(resolve_value("var(--w)", &v), "12px");
    }

    #[test]
    fn format_number_trims_and_normalizes_zero() {
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(4.5), "4.5");
        assert_eq!(format_number(-1.0), "-1");
        // negative zero normalizes so two resolution paths produce identical baked literals
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(0.0), "0");
    }

    #[test]
    fn split_pseudo_extracts_before_and_after() {
        // single selector with ::after
        let s = split_pseudo("label::after").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].0, "label");
        assert_eq!(s[0].1, "after");

        // CSS2 single-colon syntax also supported
        let s = split_pseudo("label:before").unwrap();
        assert_eq!(s[0].0, "label");
        assert_eq!(s[0].1, "before");

        // pseudo-class preceding the pseudo-element does not confuse the splitter
        let s = split_pseudo("input:checked + label::after").unwrap();
        assert_eq!(s[0].0, "input:checked + label");
        assert_eq!(s[0].1, "after");

        // no pseudo-element -> None
        assert!(split_pseudo("label:hover").is_none());
        assert!(split_pseudo("button").is_none());
    }

    #[test]
    fn scan_rule_blocks_skips_comments_and_at_rules() {
        let css = "/* leading comment */\n\
                   button { color: red; /* inline */ }\n\
                   @media screen { .x { color: blue; } }\n\
                   .y { color: green; }";
        let rules = scan_rule_blocks(css);
        // at-rule block must not appear as a rule; the two real rules survive
        let sels: Vec<&str> = rules.iter().map(|(s, _)| s.trim()).collect();
        assert!(sels.contains(&"button"));
        assert!(sels.contains(&".y"));
        assert!(!sels.iter().any(|s| s.contains("@media")));
        assert!(!sels.iter().any(|s| s.contains(".x")));
    }

    #[test]
    fn preprocess_resolves_var_calc_and_strips_root_and_pseudo() {
        // An end-to-end pre-pass check: the cleaned text has no `:root`, no `::before`, no
        // `var(`, no `calc(` — every value is a concrete literal ready for simplecss.
        let css = "\
            :root { --face: #dfdfdf; --bevel: 1px; }\n\
            button { background: var(--face); box-shadow: inset calc(var(--bevel) * -1) 0 #000; }\n\
            label::before { content: \"\"; }";
        let pre = preprocess_stylesheet(css);

        // :root extracted
        assert_eq!(pre.vars.get("--face").unwrap(), "#dfdfdf");
        assert_eq!(pre.vars.get("--bevel").unwrap(), "1px");
        // ::before rule peeled off
        assert_eq!(pre.pseudo.len(), 1);
        assert_eq!(pre.pseudo[0].base, "label");
        assert_eq!(pre.pseudo[0].pseudo, "before");
        // cleaned text has the resolved literals and none of the Lab 5 syntax
        assert!(pre.cleaned.contains("background: #dfdfdf"));
        assert!(pre.cleaned.contains("inset -1px 0 #000"));
        assert!(!pre.cleaned.contains(":root"));
        assert!(!pre.cleaned.contains("::before"));
        assert!(!pre.cleaned.contains("var("));
        assert!(!pre.cleaned.contains("calc("));
    }

    // --- Lab 6: component invocation ---

    #[test]
    fn snake_case_basic() {
        assert_eq!(to_snake_case("XpButton"), "xp_button");
        assert_eq!(to_snake_case("XpGroupBox"), "xp_group_box");
        assert_eq!(to_snake_case("Button"), "button");
        assert_eq!(to_snake_case("X"), "x");
        assert_eq!(to_snake_case("Already_snake"), "already_snake");
        assert_eq!(to_snake_case("lowercase"), "lowercase");
    }

    #[test]
    fn snake_case_acronyms_split() {
        // Acronym-heavy names split per-letter (HTMLParser -> h_t_m_l_parser). Acceptable for the
        // lab; component names tend to be CamelCase. Documented in the macro doc comment.
        assert_eq!(to_snake_case("HTMLParser"), "h_t_m_l_parser");
        assert_eq!(to_snake_case("URL"), "u_r_l");
    }

    #[test]
    fn pascal_case_round_trips_snake() {
        assert_eq!(to_pascal_case("xp_button"), "XpButton");
        assert_eq!(to_pascal_case("xp_group_box"), "XpGroupBox");
        assert_eq!(to_pascal_case("button"), "Button");
        assert_eq!(to_pascal_case("a"), "A");
        assert_eq!(to_pascal_case(""), "");
        // Underscore-led / doubled underscore stay tidy: leading underscores dropped, doubles
        // collapse to a single capitalization boundary.
        assert_eq!(to_pascal_case("_leading"), "Leading");
        assert_eq!(to_pascal_case("double__underscore"), "DoubleUnderscore");
    }

    // Parse a component invocation and check the macro emits the expected call shape. We don't
    // invoke emit_component directly (it needs a full Component parse); instead parse a small
    // rsx body and check the emitted token stream contains the expected pieces. This mirrors how
    // the rest of the macro is exercised (full expansion via the demo crate's render tests).
    #[test]
    fn component_emits_function_call() {
        use dioxus_rsx::CallBody;
        let input: TokenStream2 = quote! {
            XpButton { label: "OK", key: "ignored" }
        };
        let parsed: CallBody = syn::parse2(input).expect("parse component");
        let roots = &parsed.body.roots;
        assert_eq!(roots.len(), 1, "one top-level component");
        match &roots[0] {
            BodyNode::Component(c) => {
                // emit_component should produce a call to xp_button with the right Props type and
                // a Default-spread. The key: field must be dropped (reserved).
                let ctx = StyleCtx { styles: None };
                let out = emit_component(c, &ctx);
                let s = out.to_string();
                assert!(s.contains("xp_button"), "snake-case fn name; got: {s}");
                assert!(
                    s.contains("XpButtonProps"),
                    "PascalCase Props struct; got: {s}"
                );
                assert!(s.contains("label"), "label field present; got: {s}");
                assert!(
                    s.contains("Default :: default") || s.contains("Default::default"),
                    "Default spread for unspecified fields; got: {s}"
                );
                assert!(
                    !s.contains("ignored"),
                    "key: value must not propagate to Props; got: {s}"
                );
                // children closure present (empty body still emits the closure with annotated type)
                assert!(
                    s.contains("& mut egui :: Ui"),
                    "closure type annotation; got: {s}"
                );
            }
            other => panic!("expected Component, got {other:?}"),
        }
    }

    // Parse a `#[component]`-decorated fn and verify expand_component emits:
    //   1. the user's body renamed to <name>_def
    //   2. a #[derive(Default)] struct <Pascal>Props with the middle args as fields
    //   3. a wrapper fn <name>(ui, props, children) that forwards to _def
    #[test]
    fn component_attribute_generates_wrapper() {
        let input: TokenStream2 = quote! {
            fn xp_counter(ui: &mut egui::Ui, count: i32, label: &'static str, children: impl FnOnce(&mut egui::Ui)) {
                ui.label(label);
                let _ = (count, children);
            }
        };
        let item: syn::ItemFn = syn::parse2(input).expect("parse fn");
        let out = expand_component(item).expect("expand");
        let s = out.to_string();

        // 1. user body preserved under the renamed fn
        assert!(s.contains("fn xp_counter_def"), "renamed user fn; got: {s}");
        assert!(s.contains("ui . label"), "user body spliced; got: {s}");

        // 2. Props struct with middle args as fields (count, label), dropping ui + children
        assert!(
            s.contains("struct XpCounterProps"),
            "Props struct; got: {s}"
        );
        assert!(s.contains("count : i32"), "count field; got: {s}");
        assert!(s.contains("label : & 'static str"), "label field; got: {s}");
        assert!(s.contains("derive"), "Default derive present; got: {s}");
        assert!(s.contains("Default"), "Default in derive list; got: {s}");

        // 3. wrapper fn named after the original, with the slice-1 signature
        assert!(s.contains("fn xp_counter"), "wrapper fn name; got: {s}");
        assert!(
            s.contains("props : XpCounterProps"),
            "wrapper takes props; got: {s}"
        );
        assert!(
            s.contains("FnOnce (& mut egui :: Ui)"),
            "wrapper takes children closure; got: {s}"
        );

        // 4. wrapper forwards ui + props.count + props.label + children
        assert!(
            s.contains("props . count"),
            "forwards props.count; got: {s}"
        );
        assert!(
            s.contains("props . label"),
            "forwards props.label; got: {s}"
        );
        assert!(
            s.contains("xp_counter_def"),
            "wrapper calls the renamed user fn; got: {s}"
        );
    }

    #[test]
    fn component_attribute_rejects_self() {
        let input: TokenStream2 = quote! {
            fn xp_bad(self, ui: &mut egui::Ui, children: impl FnOnce(&mut egui::Ui)) {}
        };
        let item: syn::ItemFn = syn::parse2(input).expect("parse fn");
        let err = expand_component(item).expect_err("self should error");
        let s = err.to_compile_error().to_string();
        assert!(s.contains("self"), "error mentions self; got: {s}");
    }

    #[test]
    fn component_attribute_rejects_too_few_args() {
        let input: TokenStream2 = quote! {
            fn xp_bad(ui: &mut egui::Ui) {}
        };
        let item: syn::ItemFn = syn::parse2(input).expect("parse fn");
        let err = expand_component(item).expect_err("single-arg should error");
        let s = err.to_compile_error().to_string();
        assert!(
            s.contains("expected at least 2 args"),
            "error message; got: {s}"
        );
    }
}
