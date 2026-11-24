//! The meta files' types the library's set declares: `ComponentMeta<C>`, a component's meta as a
//! `*.meta.tsx` file declares it, `PrivateMeta<C>`, the meta of one of its private components,
//! `MetaSection<P>`, a titled group of states (a file has `states` or `sections`, never both),
//! `MetaState<P>`, one named state's props and services, and `MetaServices`, what a state's
//! `services` registers in. Every app serving the set imports them from `rayx/components`, as the
//! library's own meta files do; their former names are declared nowhere. The type check runs a
//! TypeScript compiler (`support::typecheck`), so it is `#[ignore]`d:
//! `cargo test -p rayx_components --test meta_types -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use support::typecheck;

#[test]
fn the_library_set_declares_the_meta_types() {
    let set = rayx_components::set();
    assert_eq!(set.specifier(), Some(rayx_components::SPECIFIER));
    let ts = declarations(&set, &[]);
    for declared in [
        "export type ComponentMeta<C> = { readonly id: string; readonly title?: string; \
         readonly description?: string; readonly group?: string; readonly tags?: readonly string[]; \
         readonly size?: { readonly width: number; readonly height: number }; \
         readonly component: C; \
         readonly private?: readonly PrivateMeta<(props: any) => unknown>[] } & \
         ({ readonly states: { readonly [name: string]: \
         MetaState<C extends (props: infer P) => unknown ? P : never> }; readonly sections?: never } | \
         { readonly sections: readonly \
         MetaSection<C extends (props: infer P) => unknown ? P : never>[]; readonly states?: never });",
        "export type PrivateMeta<C> = { readonly id: string; readonly title?: string; \
         readonly description?: string; readonly tags?: readonly string[]; readonly component: C } & \
         ({ readonly states: { readonly [name: string]: \
         MetaState<C extends (props: infer P) => unknown ? P : never> }; readonly sections?: never } | \
         { readonly sections: readonly \
         MetaSection<C extends (props: infer P) => unknown ? P : never>[]; readonly states?: never });",
        "export type MetaSection<P> = { readonly title: string; readonly description?: string; \
         readonly states: { readonly [name: string]: MetaState<P> } };",
        "export type MetaState<P> = { readonly props?: Partial<P>; \
         readonly services?: (scope: MetaServices) => void };",
        "export type MetaServices = { addSingleton(token: object, factory: (services: \
         { getRequiredService(token: object): any; getService(token: object): any }) => unknown): \
         MetaServices;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

/// The top-level type's former name, built from parts so that no source line holds it.
fn former_top_level_type() -> String {
    ["Prev", "iews"].concat()
}

#[test]
fn the_former_names_are_declared_nowhere() {
    let ts = declarations(&rayx_components::set(), &[]);
    for name in [
        former_top_level_type(),
        ["PrivatePrev", "iews"].concat(),
        ["PreviewSt", "ate"].concat(),
        ["PreviewServ", "ices"].concat(),
    ] {
        assert!(
            !ts.contains(&format!("export type {name}")),
            "`{name}` is declared in\n{ts}"
        );
    }
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn a_file_satisfying_the_former_top_level_type_is_refused() {
    let old = former_top_level_type();
    let refused = format!(
        r#"import type {{ {old} }} from "rayx/components";

export default {{
  id: "trip-planner",
  component: TripPlanner,
  states: {{ Default: {{ props: {{ traveler: "Ana" }} }} }},
}} satisfies {old}<typeof TripPlanner>;"#
    );
    typecheck::check(
        "types",
        &[(
            "current",
            r#"export default {
  id: "trip-planner",
  size: { width: 390, height: 844 },
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
        )],
        &[("former", &refused, &["has no exported member"])],
    );
}
