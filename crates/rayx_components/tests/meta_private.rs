//! The previews of a component's private components: components written in the previewed
//! component's file for its own use, exported only for the meta file to import. They are the
//! entries of its previews' `private` list, each written `{ … } satisfies PrivateMeta<typeof
//! Part>`, which checks its states' props against its own component. An entry has an `id`, a
//! `title`, its `component` and `states`, and no `group`, `size` or `private` of its own. The
//! type checks run a TypeScript compiler (`support::typecheck`), so they are `#[ignore]`d:
//! `cargo test -p rayx_components --test meta_private -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use support::typecheck;

/// How TypeScript prints a private entry's type (`PrivateMeta<C>`, whose either-or of `states`
/// and `sections` it spells out rather than naming the alias): its members up to `component`,
/// `tags` followed by `component` where a component's own type has `group` and `size` between.
macro_rules! private_entry_type {
    () => {
        "'{ readonly id: string; readonly title?: string | undefined; \
         readonly description?: string | undefined; \
         readonly tags?: readonly string[] | undefined; readonly component: "
    };
}

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
        declared("ComponentMeta<C>")
            .contains("readonly private?: readonly PrivateMeta<(props: any) => unknown>[] }")
    );
    let private = declared("PrivateMeta<C>");
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
    { id: "hero", component: Hero, states: { Wide: { props: { destination: "Lisbon", wide: true } } } } satisfies PrivateMeta<typeof Hero>,
    { id: "stat", title: "Stat", component: Stat, states: { Days: { props: { value: "3", label: "days" } } } } satisfies PrivateMeta<typeof Stat>,
    { id: "divider", component: Divider, states: { Default: {} } } satisfies PrivateMeta<typeof Divider>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            (
                "no-parts",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            (
                "left-out",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
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
} satisfies ComponentMeta<typeof TripPlanner>;"#,
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
    { id: "hero", component: Hero, states: { Wide: { props: { destinaton: "Lisbon" } } } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["'destinaton' does not exist in type"],
            ),
            (
                "a-wrong-value",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { destination: 7 } } } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'number' is not assignable to type 'string'"],
            ),
            (
                "the-parents-props",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Hero, states: { Wide: { props: { traveler: "Ana" } } } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["'traveler' does not exist in type"],
            ),
            (
                "another-component",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", component: Stat, states: { Wide: { props: { destination: "Lisbon" } } } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
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
      private: [{ id: "stat", component: Stat, states: { Days: {} } } satisfies PrivateMeta<typeof Stat>],
    } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &[concat!(
                    "'private' does not exist in type ",
                    private_entry_type!()
                )],
            ),
            (
                "nested-unchecked",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: Hero, states: { Wide: {} }, private: [] }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &[concat!(
                    "'private' does not exist in type ",
                    private_entry_type!()
                )],
            ),
            (
                "a-group",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", group: "Parts", component: Hero, states: { Wide: {} } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &[concat!(
                    "'group' does not exist in type ",
                    private_entry_type!()
                )],
            ),
            (
                "a-size",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    { id: "hero", size: { width: 390, height: 200 }, component: Hero, states: { Wide: {} } } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &[concat!(
                    "'size' does not exist in type ",
                    private_entry_type!()
                )],
            ),
            (
                "no-id",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ component: Hero, states: { Wide: {} } } satisfies PrivateMeta<typeof Hero>],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Property 'id' is missing"],
            ),
            (
                "no-states",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: Hero } satisfies PrivateMeta<typeof Hero>],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                // Neither `states` nor `sections`: TypeScript names the last of the two it tried.
                &[
                    "is not assignable to type 'PrivateMeta<",
                    "Property 'sections' is missing",
                ],
            ),
            (
                "not-a-list",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: { id: "hero", component: Hero, states: { Wide: {} } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["does not exist in type 'readonly PrivateMeta<"],
            ),
            (
                "not-a-component",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [{ id: "hero", component: "Hero", states: { Wide: {} } }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type '(props: any) => unknown'"],
            ),
        ],
    );
}
