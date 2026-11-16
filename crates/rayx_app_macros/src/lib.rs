//! Procedural macros for `rayx_app`: the `AppService` derive, `#[app_extension]` and
//! `#[contract]`, and the umbrella's `#[native_module]`.

mod contract;
mod extension;
mod native_module;

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{Attribute, DeriveInput, parse_macro_input};

/// Exports a Rust extension of the app builder to TS: `#[app_extension] fn add_weather(builder:
/// RayXAppBuilder) -> RayXAppBuilder` stays a plain Rust function (a Rust root calls it, or
/// `builder.apply(add_weather)`), and gains a TS function `addWeather(builder:
/// RayXAppBuilder): RayXAppBuilder` that applies it to a TS builder, which TS passes to
/// `builder.apply(addWeather)`.
///
/// The TS function is a flat NSH module named like the function, which the app registers in
/// its set: `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) {
/// registry.module::<add_weather>(); }`.
/// The function's doc comment documents the TS function.
#[proc_macro_attribute]
pub fn app_extension(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    if !attr.is_empty() {
        return syn::Error::new_spanned(attr, "`#[app_extension]` takes no arguments")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(item as syn::ItemFn);
    extension::expand(item, &app_path())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Makes a Rust trait implementable from TS while Rust keeps using it as `dyn Trait`
/// (`Arc<dyn WeatherSource>`):
///
/// ```ignore
/// #[rayx::contract]
/// pub trait WeatherSource {
///     /// The forecast for `city`.
///     async fn forecast(&self, city: String) -> Result<String, WeatherError>;
///     /// Notes `line` (posted to JS when Rust calls it outside a call from JS).
///     fn note(&self, line: String);
/// }
/// ```
///
/// Its methods are `async fn .. -> Result<T, E>` (awaited in JS's next turn; `E:
/// From<ContractError> + From<Exception>`, `String` among them) or `fn ..(&self, ..)` returning
/// nothing (run on the spot inside a call from JS, posted in order otherwise), with values that
/// cross to JS (`String`, numbers, `#[derive(JsObject)]` structs); a contract has no generics,
/// associated items, supertraits or default bodies. The trait gains `Send + Sync` and each
/// `async fn` becomes a method returning a `LocalBoxFuture`, as `async-trait(?Send)` writes it.
///
/// TS gets the interface `WeatherSource` and the token function `weatherSourceToken()`, which
/// the app adds to its set (`registry.contract::<dyn WeatherSource>()`, with
/// `rayx::app::ts::ContractRegistryExt`); `builder.addSingletonAs(weatherSourceToken(), () =>
/// new OpenMeteo())` (and the scoped, transient and startup forms) registers a TS
/// implementation, which Rust resolves as `Arc<dyn WeatherSource>` on the engine thread (off it,
/// a method fails with `ContractError::WrongThread`).
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    if !attr.is_empty() {
        return syn::Error::new_spanned(attr, "`#[contract]` takes no arguments")
            .into_compile_error()
            .into();
    }
    let item = parse_macro_input!(item as syn::ItemTrait);
    contract::expand(item, &app_path())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The UI set of an app's crate, on the function NSH's `#[native_module]` takes:
///
/// ```ignore
/// #[rayx::native_module("my-app")]
/// fn native(registry: &mut Registry) {
///     registry.register::<Glyph>().module::<add_weather>(); // the app's own exports
/// }
/// ```
///
/// It is NSH's `#[native_module("my-app")]` on the function, whose body first registers every
/// RayX TS class (`rayx::app::register`) and RayX's components with their theme document
/// (`rayx::components::register`), and nothing else; the crate gains `pub fn registry()`, as
/// with NSH's attribute. Written in the crate's source, the attribute line is what NSH's CLI
/// reads the set's specifier from. Used through the `rayx` umbrella, which it names. It takes
/// NSH's core from the crate's own `nsh-core` dependency when the crate names one (else through
/// RayX), so a crate with `nsh-core` only in `[dev-dependencies]` fails its normal build: depend
/// on it in `[dependencies]`, or not at all.
#[proc_macro_attribute]
pub fn native_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let specifier = parse_macro_input!(attr as syn::LitStr);
    let item = parse_macro_input!(item as syn::ItemFn);
    let rayx = match crate_name("rayx") {
        Ok(FoundCrate::Name(name)) => {
            let name = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        _ => quote!(::rayx),
    };
    let nsh_core_in_scope = crate_name("nsh-core").is_err();
    native_module::expand(specifier, item, &rayx, nsh_core_in_scope)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The path of `rayx_app` as the calling crate names it: through the `rayx` umbrella
/// (`::rayx::app`), else `rayx_app` itself (`::rayx_app`, which names itself so in its own
/// tests).
fn app_path() -> proc_macro2::TokenStream {
    app_path_from(|name| crate_name(name).ok())
}

fn app_path_from(found: impl Fn(&str) -> Option<FoundCrate>) -> proc_macro2::TokenStream {
    let ident = |name: &str| syn::Ident::new(name, proc_macro2::Span::call_site());
    match found("rayx") {
        Some(FoundCrate::Itself) => quote!(crate::app),
        Some(FoundCrate::Name(name)) => {
            let name = ident(&name);
            quote!(::#name::app)
        }
        None => match found("rayx_app") {
            Some(FoundCrate::Name(name)) => {
                let name = ident(&name);
                quote!(::#name)
            }
            _ => quote!(::rayx_app),
        },
    }
}

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
    fn the_app_layer_is_named_through_the_umbrella_first() {
        let app = |crates: &[(&str, FoundCrate)]| {
            app_path_from(|name| {
                crates
                    .iter()
                    .find(|(candidate, _)| *candidate == name)
                    .map(|(_, found)| found.clone())
            })
            .to_string()
        };
        assert_eq!(
            app(&[("rayx_app", named("rayx_app")), ("rayx", named("rayx"))]),
            quote!(::rayx::app).to_string()
        );
        assert_eq!(
            app(&[("rayx_app", FoundCrate::Itself)]),
            quote!(::rayx_app).to_string()
        );
        assert_eq!(
            app(&[("rayx_app", named("app_layer"))]),
            quote!(::app_layer).to_string()
        );
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
