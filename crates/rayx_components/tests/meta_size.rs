//! A component's `size` in its previews: one with no natural size (a screen, which fills what it
//! is given) declares the size, in points, a tool shows it at, a literal `{ width, height }` at
//! the top of its `ComponentMeta<C>`; a component with a natural size leaves it out, and its file type
//! checks as before. The type checks run a TypeScript compiler (`support::typecheck`), so they
//! are `#[ignore]`d: `cargo test -p rayx_components --test meta_size -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use nsh_core::element::declarations;
use support::typecheck;

/// The declaration of the alias `name` in the library's set.
fn declared(name: &str) -> String {
    let ts = declarations(&rayx_components::set(), &[]);
    let start = format!("export type {name} = ");
    ts.lines()
        .find(|line| line.starts_with(&start))
        .unwrap_or_else(|| panic!("no `{start}` in\n{ts}"))
        .to_owned()
}

#[test]
fn previews_take_an_optional_size_and_private_previews_none() {
    let size = "readonly size?: { readonly width: number; readonly height: number };";
    assert!(declared("ComponentMeta<C>").contains(size));
    assert!(!declared("PrivateMeta<C>").contains("size"));
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn a_size_is_a_literal_width_and_height() {
    typecheck::check(
        "size",
        &[
            (
                "screen",
                r#"export default {
  id: "trip-planner",
  title: "Trip planner",
  group: "Screens",
  size: { width: 390, height: 844 },
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } }, Week: { props: { traveler: "Ana", days: 7 } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            (
                "fractions",
                r#"export default {
  id: "trip-planner",
  size: { width: 390.5, height: 0 },
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ],
        &[
            (
                "a-string",
                r#"export default {
  id: "trip-planner",
  size: { width: "390", height: 844 },
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type 'number'"],
            ),
            (
                "no-height",
                r#"export default {
  id: "trip-planner",
  size: { width: 390 },
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Property 'height' is missing"],
            ),
            (
                "a-third-member",
                r#"export default {
  id: "trip-planner",
  size: { width: 390, height: 844, depth: 1 },
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["'depth' does not exist in type"],
            ),
            (
                "a-number",
                r#"export default {
  id: "trip-planner",
  size: 390,
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'number' is not assignable to type '{ readonly width: number;"],
            ),
            (
                "of-a-state",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { size: { width: 390, height: 844 } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["'size' does not exist in type"],
            ),
        ],
    );
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn a_preview_without_a_size_checks_as_before() {
    typecheck::check(
        "no-size",
        &[(
            "plain",
            r#"export default {
  id: "trip-planner",
  title: "Trip planner",
  group: "Screens",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } }, Empty: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
        )],
        &[
            (
                "a-wrong-prop",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { travelr: "Ana" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["'travelr' does not exist in type"],
            ),
            (
                "a-wrong-value",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana", days: "three" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type 'number'"],
            ),
            (
                "no-id",
                r#"export default {
  component: TripPlanner,
  states: { Default: {} },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Property 'id' is missing"],
            ),
        ],
    );
    // RayX's own meta file, as it ships.
    let button = include_str!("../src/button.meta.tsx").to_owned();
    let errors = typecheck::errors("button", &[("button.meta.tsx".to_owned(), button)]);
    assert!(errors.is_empty(), "{errors:#?}");
}
