//! `#[derive(ComponentProps)]` reads a props struct the way serde will and emits its
//! wire schema: each field's wire name, wire type, and whether it is required.
//! The worker encodes props positionally from this schema, so it must agree
//! with the `Deserialize` derive on the same struct; `gpui_react::wire::verify`
//! checks that at test time.
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, LitStr, PathArguments, Type, parse_macro_input};

#[proc_macro_derive(ComponentProps, attributes(wire))]
pub fn derive_wire_props(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Default)]
struct ContainerAttrs {
    all_default: bool,
    camel_case: bool,
    krate: Option<syn::Path>,
}
#[derive(Default)]
struct FieldAttrs {
    skip: bool,
    default: bool,
    rename: Option<String>,
    flatten: bool,
}

fn container_attrs(input: &DeriveInput) -> syn::Result<ContainerAttrs> {
    let mut attrs = ContainerAttrs::default();
    for attr in &input.attrs {
        if attr.path().is_ident("serde") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("default") {
                    attrs.all_default = true;
                    if meta.input.peek(syn::Token![=]) {
                        meta.value()?.parse::<LitStr>()?;
                    }
                } else if meta.path.is_ident("rename_all") {
                    let value: LitStr = meta.value()?.parse()?;
                    match value.value().as_str() {
                        "camelCase" => attrs.camel_case = true,
                        other => return Err(meta.error(format!("ComponentProps supports rename_all = \"camelCase\" only, not {other:?}"))),
                    }
                } else if meta.path.is_ident("deny_unknown_fields") {
                } else if meta.path.is_ident("rename_all_fields") || meta.path.is_ident("tag") || meta.path.is_ident("untagged") {
                    return Err(meta.error("ComponentProps is for plain props structs"));
                } else {
                    // Other serde container attributes do not change the field list.
                    if meta.input.peek(syn::Token![=]) {
                        meta.value()?.parse::<proc_macro2::TokenTree>()?;
                    }
                }
                Ok(())
            })?;
        } else if attr.path().is_ident("wire") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    let value: LitStr = meta.value()?.parse()?;
                    attrs.krate = Some(value.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("unknown wire attribute"))
                }
            })?;
        }
    }
    Ok(attrs)
}

fn field_attrs(field: &syn::Field) -> syn::Result<FieldAttrs> {
    let mut attrs = FieldAttrs::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") || meta.path.is_ident("skip_deserializing") {
                attrs.skip = true;
            } else if meta.path.is_ident("default") {
                attrs.default = true;
                if meta.input.peek(syn::Token![=]) {
                    meta.value()?.parse::<LitStr>()?;
                }
            } else if meta.path.is_ident("rename") {
                let value: LitStr = meta.value()?.parse()?;
                attrs.rename = Some(value.value());
            } else if meta.path.is_ident("flatten") {
                attrs.flatten = true;
            } else if meta.input.peek(syn::Token![=]) {
                meta.value()?.parse::<proc_macro2::TokenTree>()?;
            }
            Ok(())
        })?;
    }
    Ok(attrs)
}

fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The wire type of a Rust field type, and whether the type is optional.
fn wire_type(ty: &Type) -> (&'static str, bool) {
    // A type that came through a `macro_rules!` fragment arrives as a group.
    let ty = match ty {
        Type::Group(group) => return wire_type(&group.elem),
        Type::Paren(paren) => return wire_type(&paren.elem),
        ty => ty,
    };
    let Type::Path(path) = ty else { return ("Value", false) };
    let Some(segment) = path.path.segments.last() else { return ("Value", false) };
    let inner = || match &segment.arguments {
        PathArguments::AngleBracketed(args) => args.args.iter().find_map(|arg| match arg {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        }),
        _ => None,
    };
    match segment.ident.to_string().as_str() {
        "Option" => match inner() {
            Some(inner) => (wire_type(inner).0, true),
            None => ("Value", true),
        },
        "Box" | "Arc" | "Rc" => match inner() {
            Some(inner) => wire_type(inner),
            None => ("Value", false),
        },
        "bool" => ("Bool", false),
        "i8" | "i16" | "i32" | "i64" | "isize" => ("I32", false),
        "u8" | "u16" | "u32" | "u64" | "usize" => ("U32", false),
        "f32" => ("F32", false),
        "f64" => ("F64", false),
        "String" | "SharedString" | "str" | "Cow" | "PathBuf" => ("Str", false),
        "Shared" => ("Style", false),
        _ => ("Value", false),
    }
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let container = container_attrs(&input)?;
    let krate = container.krate.clone().unwrap_or_else(|| syn::parse_quote!(::gpui_react));
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
            Fields::Unit => Vec::new(),
            Fields::Unnamed(_) => return Err(syn::Error::new_spanned(&input.ident, "ComponentProps needs named fields")),
        },
        _ => return Err(syn::Error::new_spanned(&input.ident, "ComponentProps is for structs")),
    };
    let mut entries = Vec::new();
    for field in fields {
        let attrs = field_attrs(field)?;
        if attrs.skip {
            continue;
        }
        if attrs.flatten {
            return Err(syn::Error::new_spanned(field, "ComponentProps does not support flattened fields"));
        }
        let ident = field.ident.as_ref().unwrap().to_string();
        let ident = ident.strip_prefix("r#").unwrap_or(&ident).to_owned();
        let name = attrs.rename.unwrap_or_else(|| if container.camel_case { camel(&ident) } else { ident });
        let (kind, optional) = wire_type(&field.ty);
        let required = !(container.all_default || attrs.default || optional);
        let kind = syn::Ident::new(kind, proc_macro2::Span::call_site());
        entries.push(quote! {
            #krate::wire::Field { name: #name, kind: #krate::wire::WireType::#kind, required: #required }
        });
    }
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #krate::wire::ComponentProps for #name #ty_generics #where_clause {
            const SCHEMA: #krate::wire::Schema = #krate::wire::Schema::Fields(&[#(#entries),*]);
        }
    })
}
