//! `#[contract]`: a Rust trait TS implements, which Rust keeps using as `dyn Trait`.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    FnArg, Ident, ItemTrait, Pat, ReturnType, TraitItem, TraitItemFn, Type, spanned::Spanned,
};

/// What every refusal says the trait's methods may be.
const SHAPE: &str = "a contract's method is `async fn name(&self, ..) -> Result<T, E>` (awaited) \
                     or `fn name(&self, ..)` returning nothing (posted)";

/// A method of the contract.
struct Method<'a> {
    f: &'a TraitItemFn,
    params: Vec<(&'a Ident, &'a Type)>,
    is_async: bool,
}

pub(crate) fn expand(item: ItemTrait, app: &TokenStream) -> syn::Result<TokenStream> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            item.generics.span(),
            "a contract has no generic parameters (TS implements it as an interface)",
        ));
    }
    if !item.supertraits.is_empty() {
        return Err(syn::Error::new(
            item.supertraits.span(),
            "a contract has no supertraits (TS implements it as an interface, which extends none): \
             declare the members here",
        ));
    }
    if let Some(unsafety) = &item.unsafety {
        return Err(syn::Error::new(
            unsafety.span(),
            "a contract is not an `unsafe trait`",
        ));
    }
    if let Some(auto) = &item.auto_token {
        return Err(syn::Error::new(auto.span(), "a contract is no auto trait"));
    }
    let mut methods = Vec::new();
    for member in &item.items {
        let TraitItem::Fn(f) = member else {
            return Err(syn::Error::new(
                member.span(),
                "a contract has methods only (no types, constants or macros)",
            ));
        };
        methods.push(method(f)?);
    }

    let name = &item.ident;
    let vis = &item.vis;
    let ts_name = name.to_string();
    let module = format_ident!("__rayx_contract_{}", name);
    let stand_in = format_ident!("Js{}", name);
    let token_module = format_ident!("{}Token", name);
    let token_fn = Ident::new(&format!("{}_token", snake(&ts_name)), Span::call_site());
    let nsh = quote!(#app::__private::nsh_core);
    let private = quote!(#app::ts::__private);
    let docs = docs_of(&item.attrs);
    let others: Vec<_> = item
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("doc"))
        .collect();

    let mut public = Vec::new();
    let mut interface = Vec::new();
    let mut adapter = Vec::new();
    for m in &methods {
        let f = m.f;
        let ident = &f.sig.ident;
        let docs = docs_of(&f.attrs);
        let names: Vec<_> = m.params.iter().map(|(name, _)| *name).collect();
        let types: Vec<_> = m.params.iter().map(|(_, ty)| *ty).collect();
        let output = &f.sig.output;
        // The interface keeps the method as written (its `#[ts("…")]` included).
        interface.push(quote!(#f));
        if m.is_async {
            let ReturnType::Type(_, ret) = output else {
                unreachable!("checked: an async method returns a Result");
            };
            public.push(quote! {
                #(#docs)*
                fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                    -> #private::LocalBoxFuture<'__rayx, #ret>;
            });
            adapter.push(quote! {
                fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                    -> #private::LocalBoxFuture<'__rayx, #ret>
                {
                    ::std::boxed::Box::pin(async move {
                        let __rayx_object = self.slot.get()?;
                        <#stand_in as #name>::#ident(&__rayx_object, #(#names),*).await
                    })
                }
            });
        } else {
            let method = ident.to_string();
            public.push(quote! {
                #(#docs)*
                fn #ident(&self, #(#names: #types),*);
            });
            adapter.push(quote! {
                fn #ident(&self, #(#names: #types),*) {
                    match self.slot.get() {
                        ::core::result::Result::Ok(__rayx_object) => {
                            <#stand_in as #name>::#ident(&__rayx_object, #(#names),*)
                        }
                        ::core::result::Result::Err(__rayx_error) => {
                            #private::dropped(__rayx_error, #method)
                        }
                    }
                }
            });
        }
    }

    let token_doc = format!(
        " The token TS registers a `{ts_name}` implementation under and resolves it by: \
         `addSingletonAs({}Token(), () => new My{ts_name}())`, `getRequiredService({}Token())`.",
        camel(&snake(&ts_name)),
        camel(&snake(&ts_name)),
    );
    let token_ts = format!("(): ServiceToken<{ts_name}>");

    Ok(quote! {
        #(#docs)*
        #(#others)*
        #vis trait #name: ::core::marker::Send + ::core::marker::Sync {
            #(#public)*
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module {
            #[allow(unused_imports)]
            use super::*;
            // NSH's macros name `nsh_core` in scope when the app's crate takes NSH through RayX.
            #[allow(unused_imports)]
            use #nsh;

            #(#docs)*
            #[#nsh::element::interface]
            #[allow(async_fn_in_trait)]
            pub trait #name {
                #(#interface)*
            }

            /// A contract's token function (`#[contract]`).
            #[#nsh::element::module]
            pub struct #token_module {}

            #[#nsh::element::module(flat)]
            impl #token_module {
                #[doc = #token_doc]
                #[ts(#token_ts)]
                pub fn #token_fn(
                    sys: #nsh::element::System,
                ) -> #nsh::element::Instance<#app::ts::ServiceToken> {
                    #app::ts::contract_token::<dyn super::#name>(&sys)
                }
            }

            /// The `Send + Sync` implementation of the contract over its TS implementation.
            pub struct Adapter {
                pub(super) slot: #private::ContractSlot<#stand_in>,
            }

            impl super::#name for Adapter {
                #(#adapter)*
            }
        }

        impl #app::ts::Contract for dyn #name {
            const NAME: &'static str = #ts_name;

            fn register(registry: &mut #nsh::element::Registry) {
                registry
                    .interface::<#module::#stand_in>()
                    .module::<#module::#token_module>();
            }

            fn adapter(source: #private::ContractSource) -> ::std::sync::Arc<Self> {
                ::std::sync::Arc::new(#module::Adapter {
                    slot: #private::ContractSlot::new(source, #ts_name, #module::#stand_in::new),
                })
            }
        }
    })
}

/// The method `f` of a contract, checked.
fn method(f: &TraitItemFn) -> syn::Result<Method<'_>> {
    let sig = &f.sig;
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            sig.generics.span(),
            "a contract's method has no generic parameters",
        ));
    }
    if let Some(body) = &f.default {
        return Err(syn::Error::new(
            body.span(),
            "a contract's method has no default body: TS implements every method",
        ));
    }
    if let Some(token) = [
        sig.constness.map(|t| t.span()),
        sig.unsafety.map(|t| t.span()),
        sig.abi.as_ref().map(Spanned::span),
    ]
    .into_iter()
    .flatten()
    .next()
    {
        return Err(syn::Error::new(token, SHAPE));
    }
    let mut inputs = sig.inputs.iter();
    match inputs.next() {
        Some(FnArg::Receiver(receiver))
            if receiver.reference.is_some() && receiver.mutability.is_none() => {}
        _ => {
            return Err(syn::Error::new(
                sig.span(),
                "a contract's method takes `&self` first",
            ));
        }
    }
    let mut params = Vec::new();
    for input in inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new(input.span(), "`self` comes first"));
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return Err(syn::Error::new(
                typed.pat.span(),
                "a contract's parameter is a plain name (`city: String`)",
            ));
        };
        params.push((&pat.ident, &*typed.ty));
    }
    let is_async = sig.asyncness.is_some();
    let returns_unit = match &sig.output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty()),
    };
    let returns_result = match &sig.output {
        ReturnType::Type(_, ty) => matches!(
            &**ty,
            Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Result")
        ),
        ReturnType::Default => false,
    };
    if is_async && !returns_result {
        return Err(syn::Error::new(
            sig.output.span(),
            format!(
                "an `async` method of a contract returns a `Result` (its error is what JS throws): {SHAPE}"
            ),
        ));
    }
    if !is_async && !returns_unit {
        return Err(syn::Error::new(
            sig.output.span(),
            format!(
                "a contract's method that is not `async` returns nothing (it is posted to JS when \
                 Rust calls it outside a call from JS): {SHAPE}"
            ),
        ));
    }
    Ok(Method {
        f,
        params,
        is_async,
    })
}

fn docs_of(attrs: &[syn::Attribute]) -> Vec<&syn::Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect()
}

/// `WeatherSource` as `weather_source`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `weather_source` as `weatherSource`.
fn camel(name: &str) -> String {
    let mut out = String::new();
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn refused(item: TokenStream) -> String {
        match expand(syn::parse2(item).unwrap(), &quote!(::rayx_app)) {
            Ok(_) => "accepted".to_owned(),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_contract_has_no_generics_supertraits_or_other_items() {
        assert!(
            refused(quote!(
                trait Source<T> {
                    fn a(&self);
                }
            ))
            .contains("no generic parameters")
        );
        assert!(
            refused(quote!(
                trait Source: Clone {
                    fn a(&self);
                }
            ))
            .contains("no supertraits")
        );
        assert!(
            refused(quote!(
                trait Source {
                    type Item;
                }
            ))
            .contains("methods only")
        );
        assert!(
            refused(quote!(
                trait Source {
                    fn a<T>(&self, t: T);
                }
            ))
            .contains("no generic parameters")
        );
        assert!(
            refused(quote!(
                trait Source {
                    fn a(&self) {}
                }
            ))
            .contains("no default body")
        );
    }

    #[test]
    fn a_method_is_awaited_with_a_result_or_posted_with_nothing() {
        assert!(
            refused(quote!(
                trait Source {
                    fn a(&self) -> u32;
                }
            ))
            .contains("returns nothing")
        );
        assert!(
            refused(quote!(
                trait Source {
                    async fn a(&self) -> u32;
                }
            ))
            .contains("returns a `Result`")
        );
        assert!(
            refused(quote!(
                trait Source {
                    fn a(self);
                }
            ))
            .contains("`&self` first")
        );
        assert_eq!(
            refused(quote!(
                trait Source {
                    async fn a(&self, city: String) -> Result<String, String>;
                    fn b(&self, line: String);
                    fn c(&self) -> ();
                }
            )),
            "accepted"
        );
    }

    #[test]
    fn the_token_function_is_named_after_the_trait() {
        assert_eq!(snake("WeatherSource"), "weather_source");
        assert_eq!(camel(&snake("WeatherSource")), "weatherSource");
    }
}
