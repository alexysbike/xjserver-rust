use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Error, ItemFn, Result};

use crate::args::ParsedArgs;

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    if !attr.is_empty() {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            "`#[xj_can_execute]` does not take attributes",
        ));
    }

    let mut func: ItemFn = syn::parse2(item)?;
    if func.sig.asyncness.is_none() {
        return Err(Error::new(
            func.sig.fn_token.span(),
            "`#[xj_can_execute]` requires `async fn`",
        ));
    }

    match &func.sig.output {
        syn::ReturnType::Type(_, ty) if is_bool(ty) => {}
        _ => {
            return Err(Error::new(
                func.sig.output.span(),
                "`#[xj_can_execute]` handlers must return `bool`",
            ));
        }
    }

    let parsed = ParsedArgs::from_fn_inputs(&func.sig.inputs)?;
    let body = &func.block;
    let vis = &func.vis;
    let ident = &func.sig.ident;
    let attrs = &func.attrs;

    let extractions = if parsed.escape_hatch {
        let arg = &parsed.args[0];
        let pat = &arg.pat;
        let ty = &arg.ty;
        quote! {
            let #pat = match <#ty as ::xjserver::extract::FromContext<__XjIn>>::from_context(__ctx) {
                ::std::result::Result::Ok(__v) => __v,
                ::std::result::Result::Err(_) => return false,
            };
        }
    } else {
        let mut stmts = Vec::new();
        for arg in parsed.ordered() {
            let pat = &arg.pat;
            let ty = &arg.ty;
            // Fail-closed on any extractor rejection (D44).
            stmts.push(quote! {
                let #pat = match <#ty as ::xjserver::extract::FromContext<__XjIn>>::from_context(__ctx) {
                    ::std::result::Result::Ok(__v) => __v,
                    ::std::result::Result::Err(_) => return false,
                };
            });
        }
        quote! { #(#stmts)* }
    };

    // Clear original inputs — we replace the whole signature.
    func.sig.inputs.clear();

    Ok(quote! {
        #(#attrs)*
        #vis async fn #ident<__XjIn>(
            __ctx: &mut ::xjserver::Context<__XjIn>,
        ) -> bool {
            #extractions
            #body
        }
    })
}

fn is_bool(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.path.is_ident("bool"))
}
