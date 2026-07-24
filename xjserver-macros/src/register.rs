use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Error, Ident, Item, Result};

/// `#[xj_register(xj|login|session|logout)]`
pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let bucket: BucketKind = syn::parse2(attr)?;
    let item: Item = syn::parse2(item)?;

    let ident = match &item {
        Item::Struct(s) => &s.ident,
        Item::Enum(e) => &e.ident,
        Item::Type(t) => &t.ident,
        other => {
            return Err(Error::new(
                other.span(),
                "`#[xj_register]` expects a struct, enum, or type alias that implements `XJRoute`",
            ));
        }
    };

    #[cfg(not(feature = "discover"))]
    {
        let _ = (bucket, ident);
        return Ok(quote! {
            ::core::compile_error!(
                "`#[xj_register]` requires the `discover` feature on `xjserver` \
                 (`xjserver = { …, features = [\"discover\"] }` or default features)"
            );
            #item
        });
    }

    #[cfg(feature = "discover")]
    {
        let bucket_variant = bucket.variant_tokens();
        Ok(quote! {
            #item

            ::xjserver::__inventory::submit! {
                ::xjserver::RouteRegistration {
                    bucket: #bucket_variant,
                    factory: || ::xjserver::erase(#ident),
                }
            }
        })
    }
}

#[derive(Clone, Copy)]
enum BucketKind {
    Xj,
    Login,
    Session,
    Logout,
}

impl BucketKind {
    #[cfg(feature = "discover")]
    fn variant_tokens(self) -> TokenStream {
        match self {
            Self::Xj => quote! { ::xjserver::RouteBucket::Xj },
            Self::Login => quote! { ::xjserver::RouteBucket::Login },
            Self::Session => quote! { ::xjserver::RouteBucket::Session },
            Self::Logout => quote! { ::xjserver::RouteBucket::Logout },
        }
    }
}

impl Parse for BucketKind {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name: Ident = input.parse()?;
        let kind = match name.to_string().as_str() {
            "xj" => Self::Xj,
            "login" => Self::Login,
            "session" => Self::Session,
            "logout" => Self::Logout,
            _ => {
                return Err(Error::new(
                    name.span(),
                    "expected bucket: `xj`, `login`, `session`, or `logout`",
                ));
            }
        };
        if !input.is_empty() {
            return Err(Error::new(
                input.span(),
                "unexpected tokens; expected only the bucket name",
            ));
        }
        Ok(kind)
    }
}
