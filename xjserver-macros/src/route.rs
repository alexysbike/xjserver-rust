use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Error, ItemFn, Result};

use crate::args::{ParsedArgs, RouteAttrs, infer_in_type, infer_out_type};

pub fn expand_route(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    expand("xj", attr, item)
}

pub fn expand_named_bucket(
    bucket: &str,
    attr: TokenStream,
    item: TokenStream,
) -> Result<TokenStream> {
    expand(bucket, attr, item)
}

fn expand(bucket: &str, attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let route_attrs: RouteAttrs = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;

    if func.sig.asyncness.is_none() {
        return Err(Error::new(
            func.sig.fn_token.span(),
            "xjserver route macros require `async fn`",
        ));
    }
    if !func.sig.generics.params.is_empty() || func.sig.generics.where_clause.is_some() {
        return Err(Error::new(
            func.sig.generics.span(),
            "xjserver route macros do not support generic handlers yet",
        ));
    }

    let parsed = ParsedArgs::from_fn_inputs(&func.sig.inputs)?;
    let in_ty = infer_in_type(&parsed)?;
    let out_ty = infer_out_type(&func.sig.output)?;

    let vis = &func.vis;
    let ident = &func.sig.ident;
    let attrs = &func.attrs;
    let body = &func.block;

    let route_name = match &route_attrs.name {
        Some(lit) => lit.value(),
        None => ident.to_string(),
    };

    let can_execute_fn = route_attrs.can_execute.as_ref();
    let register = route_attrs.register;

    let extractions = build_execute_extractions(&parsed);

    let can_execute_method = if let Some(gate) = can_execute_fn {
        quote! {
            async fn can_execute(
                &self,
                __ctx: &mut ::xjserver::Context<Self::In>,
            ) -> bool {
                #gate(__ctx).await
            }
        }
    } else {
        quote! {}
    };

    let inventory_submit = inventory_submit_tokens(bucket, ident, register);

    Ok(quote! {
        #(#attrs)*
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, Default)]
        #vis struct #ident;

        #[::xjserver::__async_trait]
        impl ::xjserver::XJRoute for #ident {
            type In = #in_ty;
            type Out = #out_ty;

            fn name(&self) -> &'static str {
                #route_name
            }

            #can_execute_method

            async fn execute(
                &self,
                __ctx: &mut ::xjserver::Context<Self::In>,
            ) -> ::std::result::Result<Self::Out, ::xjserver::XJError> {
                #extractions
                #body
            }
        }

        #inventory_submit
    })
}

fn inventory_submit_tokens(bucket: &str, ident: &syn::Ident, register: bool) -> TokenStream {
    if !register {
        return quote! {};
    }

    #[cfg(not(feature = "discover"))]
    {
        let _ = (bucket, ident);
        return quote! {};
    }

    #[cfg(feature = "discover")]
    {
        let bucket_variant = match bucket {
            "xj" => quote! { ::xjserver::RouteBucket::Xj },
            "login" => quote! { ::xjserver::RouteBucket::Login },
            "session" => quote! { ::xjserver::RouteBucket::Session },
            "logout" => quote! { ::xjserver::RouteBucket::Logout },
            other => {
                return Error::new(
                    ident.span(),
                    format!("internal: unknown bucket `{other}`"),
                )
                .to_compile_error();
            }
        };

        quote! {
            ::xjserver::__inventory::submit! {
                ::xjserver::RouteRegistration {
                    bucket: #bucket_variant,
                    factory: || ::xjserver::erase(#ident),
                }
            }
        }
    }
}

fn build_execute_extractions(parsed: &ParsedArgs) -> TokenStream {
    let mut stmts = Vec::new();
    for arg in parsed.ordered() {
        let pat = &arg.pat;
        let ty = &arg.ty;
        stmts.push(quote! {
            let #pat = <#ty as ::xjserver::extract::FromContext<Self::In>>::from_context(__ctx)
                .map_err(::std::convert::Into::<::xjserver::XJError>::into)?;
        });
    }
    quote! { #(#stmts)* }
}
