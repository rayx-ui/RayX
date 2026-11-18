//! `#[native_module("spec")]`: an app crate's UI set with RayX's in it.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, LitStr, Pat};

/// NSH's `#[native_module(specifier)]` on `item`, whose body first calls the umbrella's
/// registration (`rayx::__private::register`, `rayx` being the umbrella's path in the calling
/// crate): RayX's TS classes, the set of every service crate the umbrella's features enable and
/// RayX's components. The registration is compiled in the umbrella, with its features. `nsh_core`
/// is brought into scope when the crate has no `nsh-core` dependency of its own, since NSH's
/// attribute then names it so.
pub(crate) fn expand(
    specifier: LitStr,
    mut item: ItemFn,
    rayx: &TokenStream,
    nsh_core_in_scope: bool,
) -> syn::Result<TokenStream> {
    let mut inputs = item.sig.inputs.iter();
    let registry = match (inputs.next(), inputs.next()) {
        (Some(FnArg::Typed(input)), None) => match &*input.pat {
            Pat::Ident(pat) => pat.ident.clone(),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "`#[rayx::native_module]`'s parameter is a name: `registry: &mut Registry`",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &item.sig,
                "`#[rayx::native_module]` goes on `fn native(registry: &mut Registry)`",
            ));
        }
    };
    let body = &item.block;
    item.block = syn::parse_quote!({
        #rayx::__private::register(#registry);
        #body
    });
    let nsh_core = if nsh_core_in_scope {
        quote! {
            #[allow(unused_imports)]
            use #rayx::app_nsh::__private::nsh_core;
        }
    } else {
        quote!()
    };
    Ok(quote! {
        #nsh_core

        #[#rayx::app_nsh::__private::nsh_core::element::native_module(#specifier)]
        #item
    })
}
