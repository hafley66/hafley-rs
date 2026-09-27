use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;
use syn::{
    parse_quote, Attribute, Expr, ExprLit, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Lit,
    MetaNameValue, Token,
};

#[derive(Default)]
struct Options {
    time_ms: Option<u64>,
    logs: Option<usize>,
    memory_bytes: Option<usize>,
}

impl Parse for Options {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let entries = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut options = Options::default();
        for entry in entries {
            let name = entry
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            let Expr::Lit(ExprLit {
                lit: Lit::Int(value),
                ..
            }) = entry.value
            else {
                return Err(syn::Error::new_spanned(
                    entry,
                    "oh budget values must be integer literals",
                ));
            };
            match name.as_str() {
                "time_ms" => options.time_ms = Some(value.base10_parse()?),
                "logs" | "logs_per_callsite" => options.logs = Some(value.base10_parse()?),
                "memory_bytes" => options.memory_bytes = Some(value.base10_parse()?),
                _ => {
                    return Err(syn::Error::new_spanned(
                        value,
                        "expected time_ms, logs, or memory_bytes",
                    ))
                }
            }
        }
        Ok(options)
    }
}

fn wrap_function(
    mut function: ItemFn,
    options: Options,
    is_test: bool,
) -> syn::Result<TokenStream> {
    if function.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            function.sig.asyncness,
            "oh attributes currently require synchronous functions",
        ));
    }
    if !function.sig.inputs.is_empty() {
        return Err(syn::Error::new_spanned(
            function.sig.inputs,
            "oh::test functions cannot take arguments",
        ));
    }
    let name = function.sig.ident.to_string();
    let time_ms = options
        .time_ms
        .map_or_else(|| quote!(None), |value| quote!(Some(#value)));
    let logs = options
        .logs
        .map_or_else(|| quote!(None), |value| quote!(Some(#value)));
    let memory_bytes = options
        .memory_bytes
        .map_or_else(|| quote!(None), |value| quote!(Some(#value)));
    let block = function.block;
    function.block = Box::new(parse_quote!({
        ::oh::testkit::run(
            #name,
            ::oh::testkit::Budget::new(#time_ms, #logs, #memory_bytes),
            || #block,
        )
    }));
    let test_attr = is_test.then(|| quote!(#[::core::prelude::v1::test]));
    Ok(quote!(#test_attr #function).into())
}

#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
    let options = parse_macro_input!(args as Options);
    let function = parse_macro_input!(input as ItemFn);
    wrap_function(function, options, true).unwrap_or_else(|error| error.into_compile_error().into())
}

#[proc_macro_attribute]
pub fn budget(args: TokenStream, input: TokenStream) -> TokenStream {
    let options = parse_macro_input!(args as Options);
    let function = parse_macro_input!(input as ItemFn);
    wrap_function(function, options, false)
        .unwrap_or_else(|error| error.into_compile_error().into())
}

#[proc_macro_attribute]
pub fn skip(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

struct InstrumentItems;

impl InstrumentItems {
    fn attrs(&self, attrs: &mut Vec<Attribute>) -> bool {
        let is_skip = |attr: &Attribute| {
            let segments = &attr.path().segments;
            segments.len() == 2 && segments[0].ident == "oh" && segments[1].ident == "skip"
        };
        let skipped = attrs.iter().any(is_skip);
        attrs.retain(|attr| !is_skip(attr));
        if !skipped {
            attrs.push(parse_quote!(#[::oh::instrument]));
        }
        skipped
    }
}

impl VisitMut for InstrumentItems {
    fn visit_item_fn_mut(&mut self, item: &mut syn::ItemFn) {
        self.attrs(&mut item.attrs);
    }

    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        for member in &mut item.items {
            if let ImplItem::Fn(function) = member {
                self.attrs(&mut function.attrs);
            }
        }
    }

    fn visit_item_mod_mut(&mut self, item: &mut ItemMod) {
        if let Some((_, items)) = &mut item.content {
            for child in items {
                self.visit_item_mut(child);
            }
        }
    }
}

#[proc_macro_attribute]
pub fn instrument_all(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item = parse_macro_input!(input as Item);
    instrument_all_item(&mut item)
        .map(|()| quote!(#item).into())
        .unwrap_or_else(|error| error.into_compile_error().into())
}

fn instrument_all_item(item: &mut Item) -> syn::Result<()> {
    match item {
        Item::Impl(item_impl) => {
            InstrumentItems.visit_item_impl_mut(item_impl);
        }
        Item::Mod(item_mod) if item_mod.content.is_some() => {
            InstrumentItems.visit_item_mod_mut(item_mod);
        }
        Item::Mod(item_mod) => {
            return Err(syn::Error::new_spanned(
                item_mod,
                "oh::instrument_all requires an inline module",
            ));
        }
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "oh::instrument_all supports inline modules and impl blocks",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::instrument_all_item;
    use syn::{parse_quote, Item};

    #[test]
    fn file_module_emits_a_named_error() {
        let mut item: Item = parse_quote!(
            mod external;
        );
        let error = instrument_all_item(&mut item).unwrap_err();
        assert_eq!(
            error.to_string(),
            "oh::instrument_all requires an inline module"
        );
    }
}
