//! Procedural macros for RayX's core (`rayx_app`) and NSH host (`rayx_app_nsh`): the `AppService`
//! derive, `#[app_extension]` and `#[contract]`, and the umbrella's `#[native_module]`.

mod contract;
mod extension;
mod native_module;

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{Attribute, DeriveInput, parse_macro_input};

/// Exports a Rust extension of the app builder to TS: `#[app_extension] fn add_weather(builder:
/// RayXAppBuilder) -> RayXAppBuilder` stays a plain Rust function (a Rust root calls it, or
/// `builder.apply(add_weather)`), and gains a TS builder method `addWeather()` on every TS
/// `RayXAppBuilder` that applies it, chaining with RayX's own methods
/// (`new RayXAppBuilder().addStorage().addWeather().start()`) and throwing once the builder
/// started.
///
/// The method is a class extension of the host's `RayXAppBuilder`, which the app registers in
/// its set: `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) {
/// registry.class_extension::<add_weather>(); }`. The function's doc comment documents the
/// method.
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
/// Its members are of five kinds:
///
/// - **commands**, `async fn m(&self, ..) -> Result<T, E>` (`E: From<ContractError> +
///   From<Exception>`, `String` among them), which become methods returning a `Send`
///   `BoxFuture`, and posted commands, `fn m(&self, ..)` returning nothing;
/// - **published state**, `#[state] fn m(&self) -> &StateCell<T>`, and **published events**,
///   `#[events] fn m(&self) -> &EventStream<E>`, which Rust reads and follows from any thread;
/// - **constants**, `#[constant] fn m(&self) -> T`, read once.
///
/// RayX links a TS implementation's state, events and constants once: a singleton's in
/// `start()` (before any Rust startup service's factory runs, which may read it), a scoped or
/// transient one's when it is resolved on the engine thread, or on its first read when it was
/// resolved on another thread, which then waits for the engine. So a
/// factory that runs off the engine thread (under the container's lock of the service it makes)
/// must not read a TS contract's state, events or constants: resolve the contract there and
/// read it afterwards, or the engine thread resolving that service at the same moment waits
/// for it.
///
/// `#[state(ts = "…")]` (and `#[events]`, `#[constant]`) writes the value's TS type where the
/// Rust type does not say it. Values cross as JS values (commands: `String`, numbers,
/// `#[derive(JsObject)]` structs) or as JSON (state, events, constants: `serde` types). A
/// contract has no generics, associated items, supertraits or default bodies; the trait gains
/// `Send + Sync`.
///
/// TS gets the interface `WeatherSource` (`#[contract(ts = "WeatherSourceContract")]` names it)
/// and the token function `weatherSourceToken()`, which the app adds to its set
/// (`registry.contract::<dyn WeatherSource>()`, with `rayx::app_nsh::ts::ContractRegistryExt`;
/// `registry.contract_class::<dyn T, C>()` makes a RayX class the token instead);
/// `builder.addSingletonAs(weatherSourceToken(), () => new OpenMeteo())` (and the scoped,
/// transient and startup forms) registers a TS implementation, which Rust resolves as
/// `Arc<dyn WeatherSource>` and calls from any thread: a call off the engine thread goes through
/// the surface's queue, and once the engine stopped it fails with `ContractError::EngineGone`.
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ts = None;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("ts") {
            ts = Some(meta.value()?.parse::<syn::LitStr>()?);
            Ok(())
        } else {
            Err(meta.error("`#[contract]` takes `ts = \"…\"` (its TS interface's name) only"))
        }
    });
    parse_macro_input!(attr with parser);
    let item = parse_macro_input!(item as syn::ItemTrait);
    let expanded = match core_path() {
        // In the core: the trait and its descriptor; a service crate makes the NSH side.
        Some(core) => contract::expand_mode(item, ts, &core, contract::Mode::Core),
        None => contract::expand_named(item, ts, &app_path()),
    };
    expanded
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The UI set of an app's crate, on the function NSH's `#[native_module]` takes:
///
/// ```ignore
/// #[rayx::native_module("my-app")]
/// fn native(registry: &mut Registry) {
///     registry.register::<Glyph>().class_extension::<add_weather>(); // the app's own exports
/// }
/// ```
///
/// It is NSH's `#[native_module("my-app")]` on the function, whose body first calls the
/// umbrella's registration (`rayx::__private::register`, compiled with the umbrella's features):
/// RayX's TS classes (`rayx::app_nsh::register`), the set of each service crate the umbrella's
/// features enable and RayX's components with their theme document
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

/// The core's path when the calling crate is the core (`rayx_app`) or reaches RayX only through
/// it (no `rayx`, no `rayx_app_nsh`): where `#[contract]` makes the trait and its descriptor
/// alone.
fn core_path() -> Option<proc_macro2::TokenStream> {
    if crate_name("rayx").is_ok() || crate_name("rayx_app_nsh").is_ok() {
        return None;
    }
    match crate_name("rayx_app").ok()? {
        FoundCrate::Itself => Some(quote!(::rayx_app)),
        FoundCrate::Name(name) => {
            let name = syn::Ident::new(&name, proc_macro2::Span::call_site());
            Some(quote!(::#name))
        }
    }
}

/// The NSH side of a contract the core defines, in a service crate (or an app):
/// `contract_nsh!(rayx_app::services::storage::KeyValueStore as KeyValueStoreContract)` makes
/// the marker type `KeyValueStoreContract`, which implements `Contract` for the trait (its TS
/// interface, the adapter over a TS implementation, its token function), with the trait's
/// values crossing as JSON. Invoke it where the trait's module is imported (`use
/// rayx_app::services::storage::*;`): the NSH side names the trait's types as the trait writes
/// them. Register the marker in the crate's set (`registry.contract::<KeyValueStoreContract>()`,
/// or `contract_class::<KeyValueStoreContract, KeyValueStore>()` with the service's class as
/// token).
#[proc_macro]
pub fn contract_nsh(input: TokenStream) -> TokenStream {
    let parser = |stream: syn::parse::ParseStream| {
        let path: syn::Path = stream.parse()?;
        stream.parse::<syn::Token![as]>()?;
        let marker: syn::Ident = stream.parse()?;
        Ok((path, marker))
    };
    let (path, marker) = parse_macro_input!(input with parser);
    let Some(last) = path.segments.last() else {
        return syn::Error::new_spanned(&path, "name the trait: `crate::services::x::Trait`")
            .into_compile_error()
            .into();
    };
    let Some(krate) = path.segments.first() else {
        unreachable!("a path has a segment");
    };
    let krate = &krate.ident;
    let descriptor = syn::Ident::new(
        &format!("__rayx_contract_descriptor_{}", last.ident),
        last.ident.span(),
    );
    let app = app_path();
    quote! {
        ::#krate::#descriptor! {
            [#app::__contract_nsh_expand] path = #path, marker = #marker,
        }
    }
    .into()
}

/// What `contract_nsh!` expands through the core's descriptor: `path = …, marker = …, ts =
/// "…"; <the trait as the core writes it>`.
#[doc(hidden)]
#[proc_macro]
pub fn __contract_nsh_expand(input: TokenStream) -> TokenStream {
    let parser = |stream: syn::parse::ParseStream| {
        let mut path = None;
        let mut marker = None;
        let mut ts = None;
        loop {
            let key: syn::Ident = stream.parse()?;
            stream.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "path" => path = Some(stream.parse::<syn::Path>()?),
                "marker" => marker = Some(stream.parse::<syn::Ident>()?),
                "ts" => ts = Some(stream.parse::<syn::LitStr>()?),
                _ => return Err(syn::Error::new(key.span(), "unknown key")),
            }
            if stream.peek(syn::Token![,]) {
                stream.parse::<syn::Token![,]>()?;
            }
            if stream.peek(syn::Token![;]) {
                stream.parse::<syn::Token![;]>()?;
                break;
            }
        }
        let item: syn::ItemTrait = stream.parse()?;
        Ok((path, marker, ts, item))
    };
    let (path, marker, ts, item) = parse_macro_input!(input with parser);
    let (Some(path), Some(marker)) = (path, marker) else {
        return syn::Error::new(proc_macro2::Span::call_site(), "missing path or marker")
            .into_compile_error()
            .into();
    };
    let ts = ts.filter(|ts| !ts.value().is_empty());
    contract::expand_mode(
        item,
        ts,
        &app_path(),
        contract::Mode::External { path, marker },
    )
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

/// The path of `rayx_app_nsh` as the calling crate names it: through the `rayx` umbrella
/// (`::rayx::app_nsh`), else `rayx_app_nsh` itself (`::rayx_app_nsh`, which names itself so in its own
/// tests).
fn app_path() -> proc_macro2::TokenStream {
    app_path_from(|name| crate_name(name).ok())
}

fn app_path_from(found: impl Fn(&str) -> Option<FoundCrate>) -> proc_macro2::TokenStream {
    let ident = |name: &str| syn::Ident::new(name, proc_macro2::Span::call_site());
    match found("rayx") {
        Some(FoundCrate::Itself) => quote!(crate::app_nsh),
        Some(FoundCrate::Name(name)) => {
            let name = ident(&name);
            quote!(::#name::app_nsh)
        }
        None => match found("rayx_app_nsh") {
            Some(FoundCrate::Name(name)) => {
                let name = ident(&name);
                quote!(::#name)
            }
            _ => quote!(::rayx_app_nsh),
        },
    }
}

/// Implements `AppService` (RayX's `RayXService`) for a service type.
///
/// The generated impl names the trait through the crate the caller depends on: the `rayx`
/// umbrella (`::rayx::app::di::AppService`), else `rayx_app` (`::rayx_app::di::AppService`),
/// else `rayx_app_nsh` (`::rayx_app_nsh::di::AppService`: the NSH layer re-exports the core under
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
            FoundCrate::Itself => quote!(crate::app::di),
            FoundCrate::Name(name) => {
                let name = ident(&name);
                quote!(::#name::app::di)
            }
        };
    }
    let path = |name: &str| {
        let name = ident(name);
        quote!(::#name::di)
    };
    // `rayx_app` declares `extern crate self as rayx_app`, so `Itself` (its own
    // unit and integration tests) resolves through the absolute path as well.
    match found("rayx_app") {
        Some(FoundCrate::Itself) => path("rayx_app"),
        Some(FoundCrate::Name(name)) => path(&name),
        None => match found("rayx_app_nsh") {
            Some(FoundCrate::Itself) => quote!(crate::di),
            Some(FoundCrate::Name(name)) => path(&name),
            None => quote!(::rayx_app::di),
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
            app(&[
                ("rayx_app_nsh", named("rayx_app_nsh")),
                ("rayx", named("rayx"))
            ]),
            quote!(::rayx::app_nsh).to_string()
        );
        assert_eq!(
            app(&[("rayx_app_nsh", FoundCrate::Itself)]),
            quote!(::rayx_app_nsh).to_string()
        );
        assert_eq!(
            app(&[("rayx_app_nsh", named("app_layer"))]),
            quote!(::app_layer).to_string()
        );
    }

    #[test]
    fn a_crate_with_only_the_umbrella_names_it() {
        assert_eq!(
            resolved(&[("rayx", named("rayx"))]),
            quote!(::rayx::app::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx", named("framework"))]),
            quote!(::framework::app::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx", FoundCrate::Itself)]),
            quote!(crate::app::di).to_string()
        );
    }

    #[test]
    fn the_umbrella_wins_over_the_layers() {
        assert_eq!(
            resolved(&[
                ("rayx_app", named("rayx_app")),
                ("rayx", named("rayx")),
                ("rayx_app_nsh", named("rayx_app_nsh")),
            ]),
            quote!(::rayx::app::di).to_string()
        );
    }

    #[test]
    fn the_core_wins_over_the_nsh_layer() {
        assert_eq!(
            resolved(&[
                ("rayx_app_nsh", named("rayx_app_nsh")),
                ("rayx_app", named("rayx_base")),
            ]),
            quote!(::rayx_base::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx_app_nsh", named("rayx_app_nsh"))]),
            quote!(::rayx_app_nsh::di).to_string()
        );
        assert_eq!(
            resolved(&[("rayx_app_nsh", FoundCrate::Itself)]),
            quote!(crate::di).to_string()
        );
        assert_eq!(resolved(&[]), quote!(::rayx_app::di).to_string());
    }
}
