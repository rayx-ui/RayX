//! `#[native_module("spec")]`: an app crate's UI set with RayX's in it.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, LitStr, Pat};

/// NSH's `#[native_module(specifier)]` on `item`, whose body first registers every RayX TS class
/// and RayX's components through `rayx` (the umbrella's path in the calling crate). `nsh_core`
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
        #rayx::app::register(#registry);
        #rayx::components::register(#registry);
        #body
    });
    let nsh_core = if nsh_core_in_scope {
        quote! {
            #[allow(unused_imports)]
            use #rayx::app::__private::nsh_core;
        }
    } else {
        quote!()
    };
    Ok(quote! {
        #nsh_core

        #[#rayx::app::__private::nsh_core::element::native_module(#specifier)]
        #item
    })
}
