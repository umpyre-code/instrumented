#![recursion_limit = "128"]

//! # Instrumented
//!
//! `instrumented` provides an attribute macro that enables instrumentation of
//! functions for use with Prometheus.
//!
//! This crate is largely based on the `log-derive` crate, and inspired by the `metered` crate.
//!
//!
extern crate proc_macro;
extern crate syn;
use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, spanned::Spanned, token, AttributeArgs, Expr, ExprBlock, ExprClosure, Ident,
    ItemFn, Meta, NestedMeta, Result, ReturnType, Type, TypePath,
};

struct FormattedAttributes {
    ok_expr: TokenStream,
    err_expr: TokenStream,
    ctx: String,
}

impl FormattedAttributes {
    pub fn parse_attributes(
        attr: &[NestedMeta],
        fmt_default: &str,
        ctx_default: &str,
    ) -> darling::Result<Self> {
        Options::from_list(attr)
            .map(|opts| Self::get_ok_err_streams(&opts, fmt_default, ctx_default))
    }

    fn get_ok_err_streams(att: &Options, fmt_default: &str, ctx_default: &str) -> Self {
        let ok_log = att.ok_log();
        let err_log = att.err_log();
        let fmt = att.fmt().unwrap_or(fmt_default);
        let ctx = att.ctx().unwrap_or(ctx_default).to_string();

        let ok_expr = match ok_log {
            Some(loglevel) => {
                let log_token = get_logger_token(&loglevel);
                quote! {log::log!(#log_token, #fmt, result);}
            }
            None => quote! {()},
        };

        let err_expr = match err_log {
            Some(loglevel) => {
                let log_token = get_logger_token(&loglevel);
                quote! {log::log!(#log_token, #fmt, err);}
            }
            None => quote! {()},
        };
        FormattedAttributes {
            ok_expr,
            err_expr,
            ctx,
        }
    }
}

#[derive(Default, FromMeta)]
#[darling(default)]
struct NamedOptions {
    ok: Option<Ident>,
    err: Option<Ident>,
    fmt: Option<String>,
    ctx: Option<String>,
}

struct Options {
    /// The log level specified as the first word in the attribute.
    leading_level: Option<Ident>,
    named: NamedOptions,
}

impl Options {
    pub fn ok_log(&self) -> Option<&Ident> {
        self.named
            .ok
            .as_ref()
            .or_else(|| self.leading_level.as_ref())
    }

    pub fn err_log(&self) -> Option<&Ident> {
        self.named
            .err
            .as_ref()
            .or_else(|| self.leading_level.as_ref())
    }

    pub fn fmt(&self) -> Option<&str> {
        self.named.fmt.as_ref().map(String::as_str)
    }

    pub fn ctx(&self) -> Option<&str> {
        self.named.ctx.as_ref().map(String::as_str)
    }
}

impl FromMeta for Options {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        if items.is_empty() {
            return Err(darling::Error::too_few_items(1));
        }

        let mut leading_level = None;

        if let NestedMeta::Meta(first) = &items[0] {
            if let Meta::Word(ident) = first {
                leading_level = Some(ident.clone());
            }
        }

        let named = if leading_level.is_some() {
            NamedOptions::from_list(&items[1..])?
        } else {
            NamedOptions::from_list(items)?
        };

        Ok(Options {
            leading_level,
            named,
        })
    }
}

/// Check if a return type is some form of `Result`. This assumes that all types named `Result`
/// are in fact results, but is resilient to the possibility of `Result` types being referenced
/// from specific modules.
pub(crate) fn is_result_type(ty: &TypePath) -> bool {
    if let Some(segment) = ty.path.segments.iter().last() {
        segment.ident == "Result"
    } else {
        false
    }
}

fn check_if_return_result(f: &ItemFn) -> bool {
    if let ReturnType::Type(_, t) = &f.decl.output {
        return match t.as_ref() {
            Type::Path(path) => is_result_type(path),
            _ => false,
        };
    }

    false
}

fn get_logger_token(att: &Ident) -> TokenStream {
    // Capitalize the first letter.
    let attr_str = att.to_string().to_lowercase();
    let mut attr_char = attr_str.chars();
    let attr_str = attr_char.next().unwrap().to_uppercase().to_string() + attr_char.as_str();
    let att_str = Ident::new(&attr_str, att.span());
    quote!(log::Level::#att_str)
}

fn make_closure(original: &ItemFn) -> ExprClosure {
    let body = Box::new(Expr::Block(ExprBlock {
        attrs: Default::default(),
        label: Default::default(),
        block: *original.block.clone(),
    }));

    ExprClosure {
        attrs: Default::default(),
        asyncness: Default::default(),
        movability: Default::default(),
        capture: Some(token::Move {
            span: original.span(),
        }),
        or1_token: Default::default(),
        inputs: Default::default(),
        or2_token: Default::default(),
        output: ReturnType::Default,
        body,
    }
}

fn replace_function_headers(original: ItemFn, new: &mut ItemFn) {
    let block = new.block.clone();
    *new = original;
    new.block = block;
}

fn generate_function(
    closure: &ExprClosure,
    expressions: &FormattedAttributes,
    result: bool,
    function_name: String,
    ctx: &str,
) -> Result<ItemFn> {
    let FormattedAttributes {
        ok_expr,
        err_expr,
        ctx,
    } = expressions;
    let code = if result {
        quote! {
            fn temp() {
                ::instrumented::inc_called_counter_for(#function_name, #ctx);
                ::instrumented::inc_inflight_for(#function_name, #ctx);
                let timer = ::instrumented::get_timer_for(#function_name, #ctx);
                (#closure)()
                    .map(|result| {
                        #ok_expr;
                        ::instrumented::dec_inflight_for(#function_name, #ctx);
                        result
                    })
                    .map_err(|err| {
                        #err_expr;
                        ::instrumented::inc_error_counter_for(#function_name, #ctx, format!("{:?}", err));
                        ::instrumented::dec_inflight_for(#function_name, #ctx);
                        err
                    })
            }
        }
    } else {
        quote! {
            fn temp() {
                ::instrumented::inc_called_counter_for(#function_name, #ctx);
                ::instrumented::inc_inflight_for(#function_name, #ctx);
                let timer = ::instrumented::get_timer_for(#function_name, #ctx);
                let result = (#closure)();
                #ok_expr;
                ::instrumented::dec_inflight_for(#function_name, #ctx);
                result
            }
        }
    };

    syn::parse2(code)
}

#[proc_macro_attribute]
pub fn instrument(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let original_fn: ItemFn = parse_macro_input!(item as ItemFn);
    let fmt_default = original_fn.ident.to_string() + "() => {:?}";
    let ctx_default = "default";
    let parsed_attributes =
        match FormattedAttributes::parse_attributes(&attr, &fmt_default, &ctx_default) {
            Ok(val) => val,
            Err(err) => {
                return err.write_errors().into();
            }
        };

    let closure = make_closure(&original_fn);
    let is_result = check_if_return_result(&original_fn);
    let mut new_fn = generate_function(
        &closure,
        &parsed_attributes,
        is_result,
        original_fn.ident.to_string(),
        &parsed_attributes.ctx,
    )
    .expect("Failed Generating Function");
    replace_function_headers(original_fn, &mut new_fn);
    new_fn.into_token_stream().into()
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::is_result_type;

    #[test]
    fn result_type() {
        assert!(is_result_type(&parse_quote!(Result<T, E>)));
        assert!(is_result_type(&parse_quote!(std::result::Result<T, E>)));
        assert!(is_result_type(&parse_quote!(fmt::Result)));
    }
}
