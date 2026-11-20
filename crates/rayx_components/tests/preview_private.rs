//! The previews of a component's private components: components written in the previewed
//! component's file for its own use, exported only for the preview file to import. They are the
//! entries of its previews' `private` list, each written `{ … } satisfies PrivatePreviews<typeof
//! Part>`, which checks its states' props against its own component. An entry has an `id`, a
//! `title`, its `component` and `states`, and no `group`, `size` or `private` of its own. The
//! type checks run a TypeScript compiler (`support::typecheck`), so they are `#[ignore]`d:
//! `cargo test -p rayx_components --test preview_private -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use support::typecheck;

#[test]
fn the_library_set_declares_private_previews_and_previews_list_them() {
    let set = rayx_components::set();
    let ts = declarations(&set, &[]);
    let declared = |name: &str| {
        let start = format!("export type {name} = ");
        ts.lines()
            .find(|line| line.starts_with(&start))
            .unwrap_or_else(|| panic!("no `{start}` in\n{ts}"))
    };
    // The list takes the previews of any component: each entry names its own.
    assert!(
        declared("Previews<C>")
            .contains("readonly private?: readonly PrivatePreviews<(props: any) => unknown>[] }")
    );
    let private = declared("PrivatePreviews<C>");
    for member in ["id", "title?", "component", "states"] {
        assert!(
            private.contains(&format!("readonly {member}: ")),
            "{member}"
        );
    }
    for member in ["group", "size", "private"] {
        assert!(!private.contains(&format!("readonly {member}")), "{member}");
    }
    assert_eq!(set.undocumented(), Vec::<String>::new());
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn each_private_entry_checks_against_its_own_component() {
    typecheck::check(
        "private",
        &[
            (
                "parts",
                r#"export default {
  id: "trip-planner",
  title: "Trip planner",
  group: "Screens",
  size: { width: 390, height: 844 },
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { destination: "Lisbon", wide: true } } } } satisfies PrivatePreviews<typeof Hero>,
    { id: "stat", title: "Stat", component: Stat, states: { Days: { props: { value: "3", label: "days" } } } } satisfies PrivatePreviews<typeof Stat>,
    { id: "divider", component: Divider, states: { Default: {} } } satisfies PrivatePreviews<typeof Divider>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "no-parts",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [],
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "left-out",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            // The list takes the previews of any component, so an entry written without its
            // own `satisfies` is not checked against its component.
            (
                "unchecked",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: Hero, states: { Wide: { props: { destinaton: 7 } } } }],
} satisfies Previews<typeof TripPlanner>;"#,
            ),
        ],
        &[
            (
                "a-wrong-prop",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { destinaton: "Lisbon" } } } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'destinaton' does not exist in type"],
            ),
            (
                "a-wrong-value",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { destination: 7 } } } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Type 'number' is not assignable to type 'string'"],
            ),
            (
                "the-parents-props",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { traveler: "Ana" } } } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'traveler' does not exist in type"],
            ),
            (
                "another-component",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Stat, states: { Wide: { props: { destination: "Lisbon" } } } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Types of parameters 'props' and 'props' are incompatible"],
            ),
        ],
    );
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn a_private_entry_is_a_preview_without_group_size_or_private() {
    typecheck::check(
        "private-shape",
        &[],
        &[
            (
                "nested",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    {
      id: "hero",
      component: Hero,
      states: { Wide: {} },
      private: [{ id: "stat", component: Stat, states: { Days: {} } } satisfies PrivatePreviews<typeof Stat>],
    } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'private' does not exist in type 'PrivatePreviews<"],
            ),
            (
                "nested-unchecked",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: Hero, states: { Wide: {} }, private: [] }],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'private' does not exist in type 'PrivatePreviews<"],
            ),
            (
                "a-group",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", group: "Parts", component: Hero, states: { Wide: {} } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'group' does not exist in type 'PrivatePreviews<"],
            ),
            (
                "a-size",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", size: { width: 390, height: 200 }, component: Hero, states: { Wide: {} } } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["'size' does not exist in type 'PrivatePreviews<"],
            ),
            (
                "no-id",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ component: Hero, states: { Wide: {} } } satisfies PrivatePreviews<typeof Hero>],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Property 'id' is missing"],
            ),
            (
                "no-states",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: Hero } satisfies PrivatePreviews<typeof Hero>],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Property 'states' is missing"],
            ),
            (
                "not-a-list",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: { id: "hero", component: Hero, states: { Wide: {} } },
} satisfies Previews<typeof TripPlanner>;"#,
                &["does not exist in type 'readonly PrivatePreviews<"],
            ),
            (
                "not-a-component",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: "Hero", states: { Wide: {} } }],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type '(props: any) => unknown'"],
            ),
        ],
    );
}
