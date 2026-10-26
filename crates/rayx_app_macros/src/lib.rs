//! Procedural macros for `rayx_app`.

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{Attribute, DeriveInput, parse_macro_input};

/// Implements `AppService` (RayX's `RayXService`) for a service type.
///
/// The generated impl names the trait as `::rayx_core::di::AppService` when the
/// calling crate depends on `rayx_core`, otherwise as `::rayx_app::di::AppService`
/// (the NSH layer re-exports the core under `di`). Renamed dependencies are honoured.
///
/// `#[app_service(startup)]` is accepted (as RayX's `#[rayx(startup)]`) and marks
/// intent only; startup registration happens through the `add_startup_*` methods.
#[proc_macro_derive(AppService, attributes(app_service))]
pub fn derive_app_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_app_service(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_app_service(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    validate_attributes(&input.attrs)?;
    let ident = input.ident;
    let di_path = di_path();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #di_path::AppService for #ident #ty_generics #where_clause {}
    })
}

fn validate_attributes(attrs: &[Attribute]) -> syn::Result<()> {
    for attr in attrs
        .iter()
        .filter(|attr| attr.path().is_ident("app_service"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("startup") {
                Ok(())
            } else {
                Err(meta.error("unsupported app_service option"))
            }
        })?;
    }
    Ok(())
}

fn di_path() -> proc_macro2::TokenStream {
    let path = |name: &str| {
        let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
        quote!(::#ident::di)
    };
    // `rayx_core` declares `extern crate self as rayx_core`, so `Itself` (its own
    // unit and integration tests) resolves through the absolute path as well.
    match crate_name("rayx_core") {
        Ok(FoundCrate::Itself) => path("rayx_core"),
        Ok(FoundCrate::Name(name)) => path(&name),
        Err(_) => match crate_name("rayx_app") {
            Ok(FoundCrate::Itself) => quote!(crate::di),
            Ok(FoundCrate::Name(name)) => path(&name),
            Err(_) => quote!(::rayx_core::di),
        },
    }
}
