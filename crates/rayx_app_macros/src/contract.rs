//! `#[contract]`: a Rust trait TS implements, which Rust keeps using as `dyn Trait`.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    FnArg, Ident, ItemTrait, Pat, ReturnType, TraitItem, TraitItemFn, Type, spanned::Spanned,
};

/// What every refusal says a contract's members may be.
const SHAPE: &str = "a contract's member is a command `async fn name(&self, ..) -> Result<T, E>` \
                     (awaited), a posted command `fn name(&self, ..)` returning nothing, \
                     published state `#[state] fn name(&self) -> &StateCell<T>`, published \
                     events `#[events] fn name(&self) -> &EventStream<E>` or a constant \
                     `#[constant] fn name(&self) -> T`";

/// What a member of the contract is.
enum Kind<'a> {
    /// `async fn m(&self, ..) -> Result<T, E>`.
    Command,
    /// `fn m(&self, ..)`.
    Posted,
    /// `#[state] fn m(&self) -> &StateCell<T>`: the cell's type and `T`.
    State { cell: &'a Type, value: &'a Type },
    /// `#[events] fn m(&self) -> &EventStream<E>`: the stream's type and `E`.
    Events { stream: &'a Type, event: &'a Type },
    /// `#[constant] fn m(&self) -> T`.
    Constant { value: &'a Type },
}

/// A member of the contract.
struct Method<'a> {
    f: &'a TraitItemFn,
    params: Vec<(&'a Ident, &'a Type)>,
    kind: Kind<'a>,
    /// The TS type of the member's value (`ts = "…"` on its attribute), for state, events and
    /// constants.
    ts: Option<String>,
}

#[cfg(test)]
pub(crate) fn expand(item: ItemTrait, app: &TokenStream) -> syn::Result<TokenStream> {
    expand_named(item, None, app)
}

/// Where a contract is expanded.
pub(crate) enum Mode {
    /// In an app's crate (or the host): the trait and its NSH side, `Contract` implemented for
    /// `dyn Trait`. `app` is the host's path.
    App,
    /// In the core (`rayx_app`, no NSH): the trait, and a hidden exported descriptor macro a
    /// service crate expands its NSH side from (`contract_nsh!`). `app` is the core's path.
    Core,
    /// A service crate's `contract_nsh!(path::Trait as Marker)`: the NSH side of a trait defined
    /// elsewhere (the core), its values crossing as JSON, `Contract` implemented for the marker.
    External { path: syn::Path, marker: Ident },
}

/// `#[contract(ts = "…")]`: the contract with its TS interface named `ts` (the trait's name when
/// `None`).
pub(crate) fn expand_named(
    item: ItemTrait,
    ts: Option<syn::LitStr>,
    app: &TokenStream,
) -> syn::Result<TokenStream> {
    expand_mode(item, ts, app, Mode::App)
}

/// The contract `item` expanded in `mode`.
pub(crate) fn expand_mode(
    item: ItemTrait,
    ts: Option<syn::LitStr>,
    app: &TokenStream,
    mode: Mode,
) -> syn::Result<TokenStream> {
    let json = matches!(mode, Mode::External { .. });
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
                format!("a contract has methods only (no types, constants or macros): {SHAPE}"),
            ));
        };
        methods.push(method(f)?);
    }

    let name = &item.ident;
    let vis = &item.vis;
    let ts_name = name.to_string();
    let iface = match &ts {
        Some(ts) => {
            let value = ts.value();
            if syn::parse_str::<Ident>(&value).is_err() {
                return Err(syn::Error::new(
                    ts.span(),
                    "`ts = \"…\"` names the TS interface: an identifier (`KeyValueStoreContract`)",
                ));
            }
            Ident::new(&value, ts.span())
        }
        None => name.clone(),
    };
    let iface_name = iface.to_string();
    let module = format_ident!("__rayx_contract_{}", name);
    let stand_in = format_ident!("Js{}", iface);
    let token_module = format_ident!("{}Token", name);
    let token_fn = Ident::new(&format!("{}_token", snake(&ts_name)), Span::call_site());
    let nsh = quote!(#app::__private::nsh_core);
    let private = quote!(#app::ts::__private);
    // The boxed future a command returns: the core's re-export in the core, the host's
    // elsewhere.
    let boxed = match mode {
        Mode::Core => quote!(#app::__private::BoxFuture),
        _ => quote!(#private::BoxFuture),
    };
    let docs = docs_of(&item.attrs);
    let others: Vec<_> = item
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("doc"))
        .collect();

    let mut public = Vec::new();
    let mut interface = Vec::new();
    let mut adapter = Vec::new();
    // The members the adapter links once, when it is made: field, type, how it is linked.
    let mut linked_fields = Vec::new();
    for m in &methods {
        let f = m.f;
        let ident = &f.sig.ident;
        let member = ident.to_string();
        let docs = docs_of(&f.attrs);
        let names: Vec<_> = m.params.iter().map(|(name, _)| *name).collect();
        let types: Vec<_> = m.params.iter().map(|(_, ty)| *ty).collect();
        let output = &f.sig.output;
        match &m.kind {
            Kind::Command if json => {
                let ReturnType::Type(_, ret) = output else {
                    unreachable!("checked: a command returns a Result");
                };
                let ok = result_ok(ret);
                let unit =
                    ok.is_some_and(|ok| matches!(ok, Type::Tuple(tuple) if tuple.elems.is_empty()));
                let ts = ts_signature(f, &m.params, ok.map(ts_result_of).as_deref());
                let ts_attrs = ts_attrs(f, &ts);
                // A command answering nothing awaits JS's `void`; one answering a value takes it
                // as JSON, its bytes as they are (a `Uint8Array` as a `number[]`).
                let (answer, to_json) = if unit {
                    (quote!(()), quote!(.map(|()| #nsh::JsonValue::Null)))
                } else {
                    (
                        quote!(#private::ContractJson),
                        quote!(.map(|__rayx_json| __rayx_json.0)),
                    )
                };
                interface.push(quote! {
                    #(#docs)*
                    #(#ts_attrs)*
                    async fn #ident(&self, #(#names: #nsh::JsonValue),*)
                        -> ::core::result::Result<#answer, ::std::string::String>;
                });
                public.push(quote! {
                    #(#docs)*
                    fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                        -> #boxed<'__rayx, #ret>;
                });
                adapter.push(quote! {
                    fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                        -> #boxed<'__rayx, #ret>
                    {
                        let __rayx_json = (#(#private::to_json(&#names),)*);
                        // The arguments live in the call queued for the engine until TS answers
                        // it (or the engine drops it), whatever the caller does with the future:
                        // what crossed as a handle (an operation target's) and TS did not take is
                        // released then.
                        let __rayx_arguments = (#(#names,)*);
                        let (#(#names,)*) = __rayx_json;
                        #private::call_json(
                            ::core::clone::Clone::clone(&self.slot),
                            #member,
                            move |__rayx_object: ::std::rc::Rc<#stand_in>| {
                                let __rayx_arguments = __rayx_arguments;
                                async move {
                                    let __rayx_answer =
                                        <#stand_in as #iface>::#ident(&__rayx_object, #(#names),*)
                                            .await
                                            #to_json;
                                    ::core::mem::drop(__rayx_arguments);
                                    __rayx_answer
                                }
                            },
                        )
                    }
                });
            }
            Kind::Posted if json => {
                let ts = ts_signature(f, &m.params, None);
                let ts_attrs = ts_attrs(f, &ts);
                interface.push(quote! {
                    #(#docs)*
                    #(#ts_attrs)*
                    fn #ident(&self, #(#names: #nsh::JsonValue),*);
                });
                public.push(quote! {
                    #(#docs)*
                    fn #ident(&self, #(#names: #types),*);
                });
                adapter.push(quote! {
                    fn #ident(&self, #(#names: #types),*) {
                        #(let #names = #private::to_json(&#names);)*
                        #private::post(
                            ::core::clone::Clone::clone(&self.slot),
                            #member,
                            move |__rayx_object: ::std::rc::Rc<#stand_in>| {
                                <#stand_in as #iface>::#ident(&__rayx_object, #(#names),*)
                            },
                        )
                    }
                });
            }
            Kind::Command => {
                // The interface keeps the method as written (its `#[ts("…")]` included).
                interface.push(quote!(#f));
                let ReturnType::Type(_, ret) = output else {
                    unreachable!("checked: a command returns a Result");
                };
                public.push(quote! {
                    #(#docs)*
                    fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                        -> #boxed<'__rayx, #ret>;
                });
                adapter.push(quote! {
                    fn #ident<'__rayx>(&'__rayx self, #(#names: #types),*)
                        -> #boxed<'__rayx, #ret>
                    {
                        #private::call(
                            ::core::clone::Clone::clone(&self.slot),
                            move |__rayx_object: ::std::rc::Rc<#stand_in>| async move {
                                <#stand_in as #iface>::#ident(&__rayx_object, #(#names),*).await
                            },
                        )
                    }
                });
            }
            Kind::Posted => {
                interface.push(quote!(#f));
                public.push(quote! {
                    #(#docs)*
                    fn #ident(&self, #(#names: #types),*);
                });
                adapter.push(quote! {
                    fn #ident(&self, #(#names: #types),*) {
                        #private::post(
                            ::core::clone::Clone::clone(&self.slot),
                            #member,
                            move |__rayx_object: ::std::rc::Rc<#stand_in>| {
                                <#stand_in as #iface>::#ident(&__rayx_object, #(#names),*)
                            },
                        )
                    }
                });
            }
            Kind::State { cell, value }
            | Kind::Events {
                stream: cell,
                event: value,
            } => {
                let is_state = matches!(m.kind, Kind::State { .. });
                let class = if is_state {
                    quote!(ContractState)
                } else {
                    quote!(ContractEvents)
                };
                let ts_class = if is_state {
                    "ContractState"
                } else {
                    "ContractEvents"
                };
                let ts = format!(
                    "(): {ts_class}<{}>",
                    m.ts.clone().unwrap_or_else(|| ts_of(value))
                );
                let link = if is_state {
                    quote!(link_state)
                } else {
                    quote!(link_events)
                };
                interface.push(quote! {
                    #(#docs)*
                    #[getter]
                    #[ts(#ts)]
                    fn #ident(&self) -> ::core::result::Result<
                        #nsh::element::Instance<#app::ts::#class>,
                        ::std::string::String,
                    >;
                });
                public.push(quote! {
                    #(#docs)*
                    fn #ident(&self) -> &#cell;
                });
                adapter.push(quote! {
                    fn #ident(&self) -> &#cell {
                        &#private::linked(self.linked(), #ts_name, #member).#ident
                    }
                });
                linked_fields.push((
                    ident,
                    quote!(#cell),
                    quote! {
                        #private::#link::<#value>(
                            <#stand_in as #iface>::#ident(&__rayx_object),
                            #ts_name,
                            #member,
                        )?
                    },
                ));
            }
            Kind::Constant { value } => {
                let ts = format!("(): {}", m.ts.clone().unwrap_or_else(|| ts_of(value)));
                interface.push(quote! {
                    #(#docs)*
                    #[getter]
                    #[ts(#ts)]
                    fn #ident(&self) -> ::core::result::Result<
                        #nsh::JsonValue,
                        ::std::string::String,
                    >;
                });
                public.push(quote! {
                    #(#docs)*
                    fn #ident(&self) -> #value;
                });
                adapter.push(quote! {
                    fn #ident(&self) -> #value {
                        ::core::clone::Clone::clone(
                            &#private::linked(self.linked(), #ts_name, #member).#ident,
                        )
                    }
                });
                linked_fields.push((
                    ident,
                    quote!(#value),
                    quote! {
                        #private::constant::<#value>(
                            <#stand_in as #iface>::#ident(&__rayx_object),
                            #ts_name,
                            #member,
                        )?
                    },
                ));
            }
        }
    }

    let field_names: Vec<_> = linked_fields.iter().map(|(name, _, _)| *name).collect();
    let field_types: Vec<_> = linked_fields.iter().map(|(_, ty, _)| ty).collect();
    let field_links: Vec<_> = linked_fields.iter().map(|(_, _, link)| link).collect();
    let has_linked = !linked_fields.is_empty();
    // An adapter of a contract that publishes links when it is made on the engine thread; made on
    // another thread (in a factory, under the container's locks) it waits for nothing there and
    // links on its first read instead.
    let link_when_made = if has_linked {
        quote! {
            if adapter.slot.is_engine_thread() {
                #private::linked_when_made(adapter.link_now(), #ts_name);
            }
        }
    } else {
        quote!()
    };
    let (linked_impl, link_body) = if !has_linked {
        (quote!(), quote!(::core::result::Result::Ok(())))
    } else {
        (
            quote! {
                impl Adapter {
                    /// Reads what the TS implementation publishes and its constants, once, on
                    /// the engine thread (in a call from JS or between JS's turns). A member that
                    /// is missing or does not convert is kept as the adapter's failure; an object
                    /// not made yet (a singleton before `start()`) is tried again later.
                    pub(super) fn link_now(&self) -> ::core::result::Result<(), #app::ts::ContractError> {
                        if let ::core::option::Option::Some(linked) = self.linked.get() {
                            return linked.as_ref().map(|_| ()).map_err(::core::clone::Clone::clone);
                        }
                        let __rayx_object = self.slot.get()?;
                        let linked = (|| -> ::core::result::Result<Linked, #app::ts::ContractError> {
                            ::core::result::Result::Ok(Linked {
                                #(#field_names: #field_links,)*
                            })
                        })();
                        self.linked.set(linked)
                    }

                    /// What the adapter linked (when it was made, or now, on the engine thread or
                    /// through it), or why it could not.
                    fn linked(&self) -> ::core::result::Result<&Linked, #app::ts::ContractError> {
                        if self.linked.get().is_none() {
                            let me = self.me.upgrade().ok_or(#app::ts::ContractError::Unavailable {
                                contract: #ts_name,
                                message: "its adapter is gone".into(),
                            })?;
                            #private::link_on_engine(&self.slot, move || me.link_now())?;
                        }
                        match self.linked.get() {
                            ::core::option::Option::Some(linked) => {
                                linked.as_ref().map_err(::core::clone::Clone::clone)
                            }
                            ::core::option::Option::None => {
                                ::core::result::Result::Err(#app::ts::ContractError::Unavailable {
                                    contract: #ts_name,
                                    message: "its TS implementation was not linked".into(),
                                })
                            }
                        }
                    }
                }
            },
            quote! {
                self.linked()
                    .map(|_| ())
                    .map_err(|__rayx_error| ::std::string::ToString::to_string(&__rayx_error))
            },
        )
    };

    let token_doc = format!(
        " The token TS registers a `{iface_name}` implementation under and resolves it by: \
         `addSingletonAs({}Token(), () => new My{ts_name}())`, `getRequiredService({}Token())`.",
        camel(&snake(&ts_name)),
        camel(&snake(&ts_name)),
    );
    let token_ts = format!("(): ServiceToken<{iface_name}>");

    let public_trait = quote! {
        #(#docs)*
        #(#others)*
        #vis trait #name: ::core::marker::Send + ::core::marker::Sync {
            #(#public)*

            /// Links what the implementation publishes and its constants, once: what RayX's
            /// adapter over a TS implementation does when `start()` makes it. Nothing for a
            /// Rust implementation.
            #[doc(hidden)]
            fn __rayx_contract_link(&self) -> ::core::result::Result<(), ::std::string::String> {
                ::core::result::Result::Ok(())
            }
        }
    };

    if let Mode::Core = mode {
        // What a service crate expands the NSH side from: the trait as written.
        let descriptor = format_ident!("__rayx_contract_descriptor_{}", name);
        let ts_value = ts.as_ref().map(syn::LitStr::value).unwrap_or_default();
        return Ok(quote! {
            #public_trait

            #[doc(hidden)]
            #[macro_export]
            macro_rules! #descriptor {
                ([$($callback:tt)*] $($args:tt)*) => {
                    $($callback)*! { $($args)* ts = #ts_value; #item }
                };
            }
        });
    }

    let (trait_ref, dyn_target, contract_target, in_module, marker_def) = match &mode {
        Mode::External { path, marker } => {
            let marker_doc = format!(
                " The contract `{ts_name}` on NSH: its TS interface `{iface_name}` and the \
                 adapter over a TS implementation. Register it in an app's set with \
                 `registry.contract::<{marker}>()`, or with its service's class as token \
                 (`registry.contract_class::<{marker}, Class>()`)."
            );
            (
                quote!(#path),
                quote!(dyn #path),
                quote!(#marker),
                quote!(super::#marker),
                quote! {
                    #[doc = #marker_doc]
                    #vis struct #marker;
                },
            )
        }
        _ => (
            quote!(super::#name),
            quote!(dyn #name),
            quote!(dyn #name),
            quote!(dyn super::#name),
            quote!(),
        ),
    };
    let public_trait = match mode {
        Mode::External { .. } => quote!(),
        _ => public_trait,
    };

    Ok(quote! {
        #public_trait
        #marker_def

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
            pub trait #iface {
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
                    #app::ts::contract_token::<#in_module>(&sys)
                }
            }

            /// What the adapter links once: the cells and streams it publishes what TS publishes
            /// into, and the constants it read.
            pub struct Linked {
                #(pub(super) #field_names: #field_types,)*
            }

            /// The `Send + Sync` implementation of the contract over its TS implementation.
            pub struct Adapter {
                pub(super) slot: #private::ContractSlot<#stand_in>,
                pub(super) linked: #private::ContractLinked<Linked>,
                /// Itself, to link through the engine from another thread.
                #[allow(dead_code)]
                pub(super) me: ::std::sync::Weak<Adapter>,
            }

            #linked_impl

            impl #trait_ref for Adapter {
                #(#adapter)*

                fn __rayx_contract_link(
                    &self,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    #link_body
                }
            }
        }

        impl #app::ts::Contract for #contract_target {
            type Dyn = #dyn_target;
            const NAME: &'static str = #ts_name;
            const INTERFACE: &'static str = #iface_name;

            fn register(registry: &mut #nsh::element::Registry) {
                registry
                    .interface::<#module::#stand_in>()
                    .module::<#module::#token_module>();
            }

            fn register_interface(registry: &mut #nsh::element::Registry) {
                registry.interface::<#module::#stand_in>();
            }

            fn adapter(source: #private::ContractSource) -> ::std::sync::Arc<Self::Dyn> {
                let adapter = ::std::sync::Arc::new_cyclic(|me| #module::Adapter {
                    slot: #private::ContractSlot::new(source, #ts_name, #module::#stand_in::new),
                    linked: #private::ContractLinked::default(),
                    me: ::std::clone::Clone::clone(me),
                });
                #link_when_made
                adapter
            }

            fn link(adapter: &Self::Dyn) -> ::core::result::Result<(), ::std::string::String> {
                adapter.__rayx_contract_link()
            }
        }
    })
}

/// The member `f` of a contract, checked.
fn method(f: &TraitItemFn) -> syn::Result<Method<'_>> {
    let sig = &f.sig;
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            sig.generics.span(),
            format!("a contract's method has no generic parameters: {SHAPE}"),
        ));
    }
    if let Some(body) = &f.default {
        return Err(syn::Error::new(
            body.span(),
            format!(
                "a contract's method has no default body (TS implements every method): {SHAPE}"
            ),
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
                format!("a contract's method takes `&self` first: {SHAPE}"),
            ));
        }
    }
    let mut params = Vec::new();
    for input in inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new(
                input.span(),
                format!("`self` comes first: {SHAPE}"),
            ));
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return Err(syn::Error::new(
                typed.pat.span(),
                format!("a contract's parameter is a plain name (`city: String`): {SHAPE}"),
            ));
        };
        params.push((&pat.ident, &*typed.ty));
    }

    // `#[state]`, `#[events]` or `#[constant]`, with an optional `ts = "…"`.
    let mut marked: Option<(&'static str, Option<String>, Span)> = None;
    for attr in &f.attrs {
        let Some(which) = ["state", "events", "constant"]
            .into_iter()
            .find(|name| attr.path().is_ident(name))
        else {
            continue;
        };
        if marked.is_some() {
            return Err(syn::Error::new(
                attr.span(),
                format!("a contract's member is one kind: {SHAPE}"),
            ));
        }
        let mut ts = None;
        if !matches!(attr.meta, syn::Meta::Path(_)) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("ts") {
                    ts = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                    Ok(())
                } else {
                    Err(meta.error(format!("`#[{which}]` takes `ts = \"…\"` only")))
                }
            })?;
        }
        marked = Some((which, ts, attr.span()));
    }

    let is_async = sig.asyncness.is_some();
    let returned = match &sig.output {
        ReturnType::Type(_, ty) => Some(&**ty),
        ReturnType::Default => None,
    };
    let returns_unit =
        returned.is_none_or(|ty| matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty()));

    if let Some((which, ts, span)) = marked {
        if is_async || !params.is_empty() {
            return Err(syn::Error::new(
                span,
                format!(
                    "`#[{which}]` goes on a member that is not `async` and takes `&self` alone: {SHAPE}"
                ),
            ));
        }
        let kind = match which {
            "state" | "events" => {
                let wrapper = if which == "state" {
                    "StateCell"
                } else {
                    "EventStream"
                };
                let Some((referent, value)) = returned.and_then(|ty| behind(ty, wrapper)) else {
                    return Err(syn::Error::new(
                        sig.output.span(),
                        format!("`#[{which}]` returns `&{wrapper}<T>`: {SHAPE}"),
                    ));
                };
                if which == "state" {
                    Kind::State {
                        cell: referent,
                        value,
                    }
                } else {
                    Kind::Events {
                        stream: referent,
                        event: value,
                    }
                }
            }
            _ => match returned {
                Some(value) if !returns_unit && !matches!(value, Type::Reference(_)) => {
                    Kind::Constant { value }
                }
                _ => {
                    return Err(syn::Error::new(
                        sig.output.span(),
                        format!("`#[constant]` returns its value, owned: {SHAPE}"),
                    ));
                }
            },
        };
        return Ok(Method {
            f,
            params,
            kind,
            ts,
        });
    }

    let returns_result = returned.is_some_and(|ty| {
        matches!(
            ty,
            Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Result")
        )
    });
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
                 Rust calls it outside a call from JS), unless it is `#[state]`, `#[events]` or \
                 `#[constant]`: {SHAPE}"
            ),
        ));
    }
    Ok(Method {
        f,
        params,
        kind: if is_async {
            Kind::Command
        } else {
            Kind::Posted
        },
        ts: None,
    })
}

/// The `T` of a `Result<T, E>`.
fn result_ok(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

/// A member's TS signature from its Rust types, as NSH's `#[ts("…")]` writes it:
/// `(city: string): Forecast`, the result left out for `void` (NSH adds the promise of an
/// `async` member).
fn ts_signature(f: &TraitItemFn, params: &[(&Ident, &Type)], result: Option<&str>) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|(name, ty)| format!("{}: {}", camel(&name.to_string()), ts_of(ty)))
        .collect();
    let _ = f;
    match result {
        Some(result) if result != "null" => format!("({}): {result}", params.join(", ")),
        _ => format!("({})", params.join(", ")),
    }
}

/// The member's own `#[ts("…")]` attributes when it has any, else one with `ts`.
fn ts_attrs(f: &TraitItemFn, ts: &str) -> Vec<TokenStream> {
    let own: Vec<TokenStream> = f
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("ts"))
        .map(|attr| quote!(#attr))
        .collect();
    if own.is_empty() {
        vec![quote!(#[ts(#ts)])]
    } else {
        own
    }
}

/// `&Wrapper<T>`'s `Wrapper<T>` and `T`, when `ty` is one.
fn behind<'a>(ty: &'a Type, wrapper: &str) -> Option<(&'a Type, &'a Type)> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    if reference.mutability.is_some() {
        return None;
    }
    let referent = &*reference.elem;
    let Type::Path(path) = referent else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let value = types.next()?;
    types.next().is_none().then_some((referent, value))
}

/// The TS type a value of `ty` crosses as (JSON): its primitives, arrays, maps and options, and
/// a named type by its name, which the app's set declares.
fn ts_of(ty: &Type) -> String {
    match ty {
        Type::Reference(reference) => ts_of(&reference.elem),
        Type::Paren(paren) => ts_of(&paren.elem),
        Type::Slice(slice) => array(&ts_of(&slice.elem)),
        Type::Array(array_ty) => array(&ts_of(&array_ty.elem)),
        Type::Tuple(tuple) if tuple.elems.is_empty() => "null".to_owned(),
        Type::Tuple(tuple) => format!(
            "[{}]",
            tuple.elems.iter().map(ts_of).collect::<Vec<_>>().join(", ")
        ),
        Type::Path(path) => {
            let Some(last) = path.path.segments.last() else {
                return "unknown".to_owned();
            };
            let args: Vec<&Type> = match &last.arguments {
                syn::PathArguments::AngleBracketed(args) => args
                    .args
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::GenericArgument::Type(ty) => Some(ty),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            match (last.ident.to_string().as_str(), args.as_slice()) {
                ("String" | "str" | "char" | "Cow", _) => "string".to_owned(),
                ("bool", _) => "boolean".to_owned(),
                (
                    "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize"
                    | "f32" | "f64",
                    _,
                ) => "number".to_owned(),
                ("Vec" | "VecDeque" | "BTreeSet" | "HashSet", [item]) => array(&ts_of(item)),
                ("Option", [inner]) => format!("{} | null", ts_of(inner)),
                ("HashMap" | "BTreeMap", [_, value]) => format!("Record<string, {}>", ts_of(value)),
                ("Box" | "Arc" | "Rc", [inner]) => ts_of(inner),
                ("JsonValue" | "Value", _) => "unknown".to_owned(),
                (name, _) => name.to_owned(),
            }
        }
        _ => "unknown".to_owned(),
    }
}

/// The TS type of a command's result, `ty`: as [`ts_of`], with bytes (`Vec<u8>`, also inside an
/// `Option`) as `number[] | Uint8Array`, both of which the adapter takes.
fn ts_result_of(ty: &Type) -> String {
    fn is_bytes(ty: &Type) -> bool {
        let Type::Path(path) = ty else {
            return false;
        };
        let Some(last) = path.path.segments.last() else {
            return false;
        };
        let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
            return false;
        };
        last.ident == "Vec"
            && matches!(
                args.args.first(),
                Some(syn::GenericArgument::Type(Type::Path(item))) if item.path.is_ident("u8")
            )
    }
    if is_bytes(ty) {
        return "number[] | Uint8Array".to_owned();
    }
    if let Type::Path(path) = ty
        && let Some(last) = path.path.segments.last()
        && last.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &last.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        && is_bytes(inner)
    {
        return "number[] | Uint8Array | null".to_owned();
    }
    ts_of(ty)
}

/// An array of `item`, parenthesized when it is a union.
fn array(item: &str) -> String {
    if item.contains('|') {
        format!("({item})[]")
    } else {
        format!("{item}[]")
    }
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
        match expand(syn::parse2(item).unwrap(), &quote!(::rayx_app_nsh)) {
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
        assert!(
            refused(quote!(
                trait Source {
                    fn a(&self) -> u32;
                }
            ))
            .contains("unless it is `#[state]`, `#[events]` or `#[constant]`")
        );
    }

    #[test]
    fn every_kind_of_member_is_accepted() {
        assert_eq!(
            refused(quote!(
                trait Theme {
                    async fn load(&self, name: String) -> Result<String, String>;
                    fn note(&self, line: String);
                    #[state]
                    fn current(&self) -> &StateCell<String>;
                    #[state(ts = "Palette")]
                    fn palette(&self) -> &rayx::app::StateCell<Palette>;
                    #[events]
                    fn changes(&self) -> &EventStream<u32>;
                    #[constant]
                    fn version(&self) -> u32;
                    #[constant(ts = "string[]")]
                    fn names(&self) -> Vec<String>;
                }
            )),
            "accepted"
        );
    }

    #[test]
    fn published_members_and_constants_have_their_shapes() {
        let shapes = [
            (
                quote!(
                    trait T {
                        #[state]
                        fn s(&self) -> StateCell<u32>;
                    }
                ),
                "returns `&StateCell<T>`",
            ),
            (
                quote!(
                    trait T {
                        #[state]
                        fn s(&self) -> &EventStream<u32>;
                    }
                ),
                "returns `&StateCell<T>`",
            ),
            (
                quote!(
                    trait T {
                        #[events]
                        fn e(&self) -> &StateCell<u32>;
                    }
                ),
                "returns `&EventStream<T>`",
            ),
            (
                quote!(
                    trait T {
                        #[state]
                        fn s(&self, x: u32) -> &StateCell<u32>;
                    }
                ),
                "takes `&self` alone",
            ),
            (
                quote!(
                    trait T {
                        #[state]
                        async fn s(&self) -> &StateCell<u32>;
                    }
                ),
                "is not `async`",
            ),
            (
                quote!(
                    trait T {
                        #[constant]
                        fn c(&self);
                    }
                ),
                "returns its value, owned",
            ),
            (
                quote!(
                    trait T {
                        #[constant]
                        fn c(&self) -> &u32;
                    }
                ),
                "returns its value, owned",
            ),
            (
                quote!(
                    trait T {
                        #[state]
                        #[constant]
                        fn c(&self) -> u32;
                    }
                ),
                "one kind",
            ),
            (
                quote!(
                    trait T {
                        #[state(name = "x")]
                        fn s(&self) -> &StateCell<u32>;
                    }
                ),
                "takes `ts = ",
            ),
        ];
        for (item, expected) in shapes {
            let message = refused(item);
            assert!(message.contains(expected), "{expected}: {message}");
        }
    }

    #[test]
    fn every_refusal_lists_the_five_shapes() {
        let members = [
            quote!(
                trait T {
                    fn a(&self) -> u32;
                }
            ),
            quote!(
                trait T {
                    type Item;
                }
            ),
            quote!(
                trait T {
                    fn a<X>(&self, x: X);
                }
            ),
            quote!(
                trait T {
                    fn a(&self) {}
                }
            ),
            quote!(
                trait T {
                    fn a(self);
                }
            ),
            quote!(
                trait T {
                    fn a(&mut self);
                }
            ),
            quote!(
                trait T {
                    fn a();
                }
            ),
            quote!(
                trait T {
                    fn a(&self, (x, y): (u8, u8));
                }
            ),
            quote!(
                trait T {
                    async fn a(&self) -> u32;
                }
            ),
            quote!(
                trait T {
                    const fn a(&self);
                }
            ),
            quote!(
                trait T {
                    #[state]
                    fn s(&self) -> StateCell<u32>;
                }
            ),
            quote!(
                trait T {
                    #[constant]
                    fn c(&self);
                }
            ),
        ];
        for member in members {
            let message = refused(member.clone());
            assert!(message.contains(SHAPE), "{member}: {message}");
        }
        let message = refused(quote!(
            trait T {
                fn a(&self) -> u32;
            }
        ));
        for shape in [
            "async fn name(&self, ..) -> Result<T, E>",
            "`fn name(&self, ..)` returning nothing",
            "#[state] fn name(&self) -> &StateCell<T>",
            "#[events] fn name(&self) -> &EventStream<E>",
            "#[constant] fn name(&self) -> T",
        ] {
            assert!(message.contains(shape), "{shape}: {message}");
        }
    }

    #[test]
    fn the_ts_argument_names_the_interface() {
        let item = quote!(
            trait Store {
                fn a(&self);
            }
        );
        let named = |ts: &str| {
            expand_named(
                syn::parse2(item.clone()).unwrap(),
                Some(syn::LitStr::new(ts, Span::call_site())),
                &quote!(::rayx_app_nsh),
            )
            .map(|tokens| tokens.to_string())
            .unwrap_or_else(|error| error.to_string())
        };
        let expanded = named("StoreContract");
        assert!(expanded.contains("pub trait StoreContract"), "{expanded}");
        assert!(expanded.contains("JsStoreContract"), "{expanded}");
        assert!(expanded.contains("trait Store"), "{expanded}");
        assert!(named("not a name").contains("an identifier"));
    }

    #[test]
    fn values_cross_as_their_ts_types() {
        let ts = |ty: TokenStream| ts_of(&syn::parse2(ty).unwrap());
        assert_eq!(ts(quote!(String)), "string");
        assert_eq!(ts(quote!(u32)), "number");
        assert_eq!(ts(quote!(bool)), "boolean");
        assert_eq!(ts(quote!(Vec<String>)), "string[]");
        assert_eq!(ts(quote!(Option<f64>)), "number | null");
        assert_eq!(ts(quote!(Vec<Option<u8>>)), "(number | null)[]");
        let result = |ty: TokenStream| ts_result_of(&syn::parse2(ty).unwrap());
        assert_eq!(result(quote!(Vec<u8>)), "number[] | Uint8Array");
        assert_eq!(
            result(quote!(Option<Vec<u8>>)),
            "number[] | Uint8Array | null"
        );
        assert_eq!(result(quote!(Vec<u16>)), "number[]");
        assert_eq!(
            ts(quote!(BTreeMap<String, bool>)),
            "Record<string, boolean>"
        );
        assert_eq!(ts(quote!(serde_json::Value)), "unknown");
        assert_eq!(ts(quote!(my::Forecast)), "Forecast");
    }

    #[test]
    fn the_token_function_is_named_after_the_trait() {
        assert_eq!(snake("WeatherSource"), "weather_source");
        assert_eq!(camel(&snake("WeatherSource")), "weatherSource");
    }
}
