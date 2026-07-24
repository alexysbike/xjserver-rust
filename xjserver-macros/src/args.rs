//! Shared argument parsing for route / can_execute macros.

use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Error, FnArg, GenericArgument, Ident, Pat, PathArguments, Result, Token, Type, TypeReference,
    punctuated::Punctuated,
};

/// Attribute args: `name = "…", can_execute = path, register = bool`
pub struct RouteAttrs {
    pub name: Option<syn::LitStr>,
    pub can_execute: Option<syn::Path>,
    /// Default `true` — emit inventory submit when feature `discover` is on.
    pub register: bool,
}

impl Default for RouteAttrs {
    fn default() -> Self {
        Self {
            name: None,
            can_execute: None,
            register: true,
        }
    }
}

impl Parse for RouteAttrs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut attrs = RouteAttrs::default();
        if input.is_empty() {
            return Ok(attrs);
        }

        let mut register_set = false;
        let vars = Punctuated::<AttrAssign, Token![,]>::parse_terminated(input)?;
        for var in vars {
            match var {
                AttrAssign::Name(lit) => {
                    if attrs.name.is_some() {
                        return Err(Error::new(lit.span(), "duplicate `name`"));
                    }
                    attrs.name = Some(lit);
                }
                AttrAssign::CanExecute(path) => {
                    if attrs.can_execute.is_some() {
                        return Err(Error::new(path.span(), "duplicate `can_execute`"));
                    }
                    attrs.can_execute = Some(path);
                }
                AttrAssign::Register(lit) => {
                    if register_set {
                        return Err(Error::new(lit.span(), "duplicate `register`"));
                    }
                    register_set = true;
                    attrs.register = lit.value;
                }
            }
        }
        Ok(attrs)
    }
}

enum AttrAssign {
    Name(syn::LitStr),
    CanExecute(syn::Path),
    Register(syn::LitBool),
}

impl Parse for AttrAssign {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        if key == "name" {
            Ok(AttrAssign::Name(input.parse()?))
        } else if key == "can_execute" {
            Ok(AttrAssign::CanExecute(input.parse()?))
        } else if key == "register" {
            Ok(AttrAssign::Register(input.parse()?))
        } else {
            Err(Error::new(
                key.span(),
                "unknown attribute; expected `name`, `can_execute`, or `register`",
            ))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    Owned,
    Borrowing,
}

pub struct HandlerArg {
    pub pat: Pat,
    pub ty: Type,
    pub kind: ArgKind,
}

pub struct ParsedArgs {
    pub args: Vec<HandlerArg>,
    /// Escape hatch: sole arg is `&mut Context<T>` / `Ctx<…>`.
    pub escape_hatch: bool,
}

impl ParsedArgs {
    pub fn from_fn_inputs(inputs: &Punctuated<FnArg, Token![,]>) -> Result<Self> {
        let mut args = Vec::with_capacity(inputs.len());
        for input in inputs {
            let FnArg::Typed(pat_ty) = input else {
                return Err(Error::new(
                    input.span(),
                    "xjserver macros do not support `self` receivers",
                ));
            };
            let kind = classify_type(&pat_ty.ty)?;
            args.push(HandlerArg {
                pat: (*pat_ty.pat).clone(),
                ty: (*pat_ty.ty).clone(),
                kind,
            });
        }

        let escape_hatch = match args.as_slice() {
            [only] if is_context_escape(&only.ty) => true,
            _ => {
                for arg in &args {
                    if is_context_escape(&arg.ty) {
                        return Err(Error::new(
                            arg.ty.span(),
                            "`Context` / `Ctx` escape hatch must be the only argument",
                        ));
                    }
                }
                false
            }
        };

        let borrowing_count = args
            .iter()
            .filter(|a| a.kind == ArgKind::Borrowing)
            .count();
        if borrowing_count > 1 {
            return Err(Error::new(
                args.iter()
                    .find(|a| a.kind == ArgKind::Borrowing)
                    .map(|a| a.ty.span())
                    .unwrap_or_else(proc_macro2::Span::call_site),
                "at most one borrowing extractor (`MetadataMut`, `Ctx`, `&mut Context<_>`); \
                 put owned extractors first",
            ));
        }

        Ok(Self {
            args,
            escape_hatch,
        })
    }

    /// Owned extractors first, then at most one borrowing extractor.
    pub fn ordered(&self) -> Vec<&HandlerArg> {
        let mut owned: Vec<&HandlerArg> = self
            .args
            .iter()
            .filter(|a| a.kind == ArgKind::Owned)
            .collect();
        owned.extend(self.args.iter().filter(|a| a.kind == ArgKind::Borrowing));
        owned
    }
}

fn classify_type(ty: &Type) -> Result<ArgKind> {
    if is_borrowing_type(ty) {
        Ok(ArgKind::Borrowing)
    } else {
        Ok(ArgKind::Owned)
    }
}

fn is_borrowing_type(ty: &Type) -> bool {
    if is_context_escape(ty) {
        return true;
    }
    path_ends_with(ty, "MetadataMut")
}

/// `&mut Context<_>` or `Ctx` / `Ctx<'_, _>` / `Ctx<_>`.
pub fn is_context_escape(ty: &Type) -> bool {
    if path_ends_with(ty, "Ctx") {
        return true;
    }
    match ty {
        Type::Reference(TypeReference {
            mutability: Some(_),
            elem,
            ..
        }) => path_ends_with(elem, "Context"),
        _ => false,
    }
}

fn path_ends_with(ty: &Type, name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == name)
}

/// Infer `In` from the first `Data<T>`, else from escape-hatch `Context`/`Ctx`, else `Empty`.
pub fn infer_in_type(parsed: &ParsedArgs) -> Result<Type> {
    if parsed.escape_hatch {
        return extract_context_in(&parsed.args[0].ty);
    }

    let mut found: Option<Type> = None;
    for arg in &parsed.args {
        if let Some(inner) = extract_data_inner(&arg.ty) {
            if let Some(prev) = &found {
                if !types_roughly_eq(prev, &inner) {
                    return Err(Error::new(
                        arg.ty.span(),
                        "conflicting `Data<T>` extractors; `In` must be unique",
                    ));
                }
            } else {
                found = Some(inner);
            }
        }
    }

    Ok(found.unwrap_or_else(|| syn::parse_quote!(::xjserver::Empty)))
}

fn extract_data_inner(ty: &Type) -> Option<Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let seg = type_path.path.segments.last()?;
    if seg.ident != "Data" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

fn extract_context_in(ty: &Type) -> Result<Type> {
    match ty {
        Type::Reference(TypeReference { elem, .. }) => extract_path_in_param(elem, "Context"),
        other => {
            if path_ends_with(other, "Ctx") {
                extract_path_in_param(other, "Ctx")
            } else {
                Err(Error::new(ty.span(), "expected `&mut Context<In>` or `Ctx<In>`"))
            }
        }
    }
}

fn extract_path_in_param(ty: &Type, name: &str) -> Result<Type> {
    let Type::Path(type_path) = ty else {
        return Err(Error::new(ty.span(), format!("expected `{name}<In>`")));
    };
    let seg = type_path.path.segments.last().ok_or_else(|| {
        Error::new(ty.span(), format!("expected `{name}<In>`"))
    })?;
    if seg.ident != name {
        return Err(Error::new(ty.span(), format!("expected `{name}<In>`")));
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(Error::new(
            ty.span(),
            format!("expected `{name}<In>` with a type parameter"),
        ));
    };

    // Ctx<'a, In> or Ctx<In> or Context<In>
    let type_args: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t),
            GenericArgument::Lifetime(_) => None,
            _ => None,
        })
        .collect();

    match type_args.as_slice() {
        [in_ty] => Ok((*in_ty).clone()),
        [_, in_ty] => Ok((*in_ty).clone()),
        _ => Err(Error::new(
            ty.span(),
            format!("expected `{name}<In>` or `{name}<'a, In>`"),
        )),
    }
}

fn types_roughly_eq(a: &Type, b: &Type) -> bool {
    // Sufficient for macro diagnostics (path equality via tokens).
    quote::ToTokens::to_token_stream(a).to_string() == quote::ToTokens::to_token_stream(b).to_string()
}

/// Infer `Out` from `-> Result<Out, …>`.
pub fn infer_out_type(ret: &syn::ReturnType) -> Result<Type> {
    let syn::ReturnType::Type(_, ty) = ret else {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            "handler must return `Result<Out, XJError>`",
        ));
    };

    let Type::Path(type_path) = ty.as_ref() else {
        return Err(Error::new(ty.span(), "handler must return `Result<Out, XJError>`"));
    };
    let seg = type_path.path.segments.last().ok_or_else(|| {
        Error::new(ty.span(), "handler must return `Result<Out, XJError>`")
    })?;
    if seg.ident != "Result" {
        return Err(Error::new(
            seg.ident.span(),
            "handler must return `Result<Out, XJError>`",
        ));
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(Error::new(ty.span(), "expected `Result<Out, XJError>`"));
    };
    let mut types = args.args.iter().filter_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    let out = types
        .next()
        .ok_or_else(|| Error::new(ty.span(), "expected `Result<Out, XJError>`"))?;
    Ok(out.clone())
}
