//! Procedural macros for `rayx_app`.

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{Attribute, DeriveInput, parse_macro_input};

/// Implements `AppService` (RayX's `RayXService`) for a service type.
///
/// The generated impl names the trait through the crate the caller depends on: the `rayx`
/// umbrella (`::rayx::core::di::AppService`), else `rayx_core` (`::rayx_core::di::AppService`),
/// else `rayx_app` (`::rayx_app::di::AppService`: the NSH layer re-exports the core under
/// `di`). Renamed dependencies are honoured.
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
    di_path_from(|name| crate_name(name).ok())
}

/// The path of the core's `di` module, given how the calling crate names its dependencies.
fn di_path_from(found: impl Fn(&str) -> Option<FoundCrate>) -> proc_macro2::TokenStream {
    let ident = |name: &str| syn::Ident::new(name, proc_macro2::Span::call_site());
    if let Some(found) = found("rayx") {
        return match found {
            FoundCrate::Itself => quote!(crate::core::di),
            FoundCrate::Name(name) => {
                let name = ident(&name);
                quote!(::#name::core::di)
            }
        };
    }
    let path = |name: &str| {
        let name = ident(name);
        quote!(::#name::di)
    };
    // `rayx_core` declares `extern crate self as rayx_core`, so `Itself` (its own
    // unit and integration tests) resolves through the absolute path as well.
    match found("rayx_core") {
        Some(FoundCrate::Itself) => path("rayx_core"),
        Some(FoundCrate::Name(name)) => path(&name),
        None => match found("rayx_app") {
            Some(FoundCrate::Itself) => quote!(crate::di),
            Some(FoundCrate::Name(name)) => path(&name),
            None => quote!(::rayx_core::di),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(crates: &[(&str, FoundCrate)]) -> String {
        di_path_from(|name| {
            crates
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, found)| found.clone())
        })
        .to_string()
    }

    fn named(name: &str) -> FoundCrate {
        FoundCrate::Name(name.to_owned())
    }

    #[test]
    fn a_crate_with_only_the_umbrella_names_it() {
        assert_eq!(
            resolved(&[("rayx", named("rayx"))]),
            quote!(::rayx::core::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx", named("framework"))]),
            quote!(::framework::core::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx", FoundCrate::Itself)]),
            quote!(crate::core::di).to_string()
        );
    }

    #[test]
    fn the_umbrella_wins_over_the_layers() {
        assert_eq!(
            resolved(&[
                ("rayx_core", named("rayx_core")),
                ("rayx", named("rayx")),
                ("rayx_app", named("rayx_app")),
            ]),
            quote!(::rayx::core::di).to_string()
        );
    }

    #[test]
    fn the_core_wins_over_the_nsh_layer() {
        assert_eq!(
            resolved(&[
                ("rayx_app", named("rayx_app")),
                ("rayx_core", named("rayx_base")),
            ]),
            quote!(::rayx_base::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx_app", named("rayx_app"))]),
            quote!(::rayx_app::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx_app", FoundCrate::Itself)]),
            quote!(crate::di).to_string()
        );
        assert_eq!(resolved(&[]), quote!(::rayx_core::di).to_string());
    }
}
