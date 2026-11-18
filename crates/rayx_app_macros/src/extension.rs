//! `#[app_extension]`: a Rust extension of the app builder, exported to TS.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, ReturnType, spanned::Spanned};

pub(crate) fn expand(item: ItemFn, app: &TokenStream) -> syn::Result<TokenStream> {
    let sig = &item.sig;
    if let Some(token) = sig.asyncness {
        return Err(syn::Error::new(
            token.span(),
            "an app extension is a plain `fn(builder: RayXAppBuilder) -> RayXAppBuilder`, not `async`",
        ));
    }
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            sig.generics.span(),
            "an app extension is not generic: `fn(builder: RayXAppBuilder) -> RayXAppBuilder`",
        ));
    }
    let mut inputs = sig.inputs.iter();
    match (inputs.next(), inputs.next()) {
        (Some(FnArg::Typed(_)), None) => {}
        _ => {
            return Err(syn::Error::new(
                sig.inputs.span(),
                "an app extension takes the builder alone: `fn(builder: RayXAppBuilder) -> RayXAppBuilder`",
            ));
        }
    }
    if matches!(sig.output, ReturnType::Default) {
        return Err(syn::Error::new(
            sig.span(),
            "an app extension returns the builder: `fn(builder: RayXAppBuilder) -> RayXAppBuilder`",
        ));
    }
    let ident = &sig.ident;
    let vis = &item.vis;
    let docs: Vec<_> = item
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect();
    let module = format_ident!("__rayx_app_extension_{}", ident);
    let nsh = quote!(#app::__private::nsh_core);
    Ok(quote! {
        #item

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module {
            #[allow(unused_imports)]
            use #nsh;
            // The class extension names the class by its own name.
            use #app::ts::RayXAppBuilder;

            /// The holder of the TS builder method `#[app_extension]` makes: register it in the
            /// app's set with `registry.class_extension::<the_function>()`.
            #[allow(non_camel_case_types)]
            pub struct #ident {}

            #[#nsh::element::class_extension(RayXAppBuilder)]
            impl #ident {
                #(#docs)*
                pub fn #ident(
                    this: #nsh::element::Instance<RayXAppBuilder>,
                ) -> ::core::result::Result<
                    #nsh::element::Instance<RayXAppBuilder>,
                    ::std::string::String,
                > {
                    RayXAppBuilder::extended(this, super::#ident)
                }
            }
        }

        #[doc(hidden)]
        #vis use #module::#ident;
    })
}
