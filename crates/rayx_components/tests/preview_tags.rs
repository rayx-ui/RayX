//! A preview's `tags`: words in lower case to find it by, a literal list of strings at the top of
//! its `Previews<C>` (`tags: ["form", "auth"]`), any number of them; a preview without tags type
//! checks as before, and a private component's previews take them too. The type checks run a
//! TypeScript compiler (`support::typecheck`), so they are `#[ignore]`d:
//! `cargo test -p rayx_components --test preview_tags -- --include-ignored`.

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
fn previews_and_private_previews_take_optional_tags() {
    assert!(declared("Previews<C>").contains("readonly tags?: readonly string[];"));
    assert!(declared("PrivatePreviews<C>").contains("readonly tags?: readonly string[];"));
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn tags_are_a_list_of_strings() {
    typecheck::check(
        "tags",
        &[
            (
                "one",
                r#"export default {
  id: "trip-planner",
  title: "Trip planner",
  group: "Screens",
  tags: ["form"],
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "several",
                r#"export default {
  id: "trip-planner",
  tags: ["form", "auth", "travel"],
  size: { width: 390, height: 844 },
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "none",
                r#"export default {
  id: "trip-planner",
  tags: [],
  component: TripPlanner,
  states: { Default: {} },
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "left-out",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
} satisfies Previews<typeof TripPlanner>;"#,
            ),
            (
                "of-a-private-component",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    {
      id: "hero",
      tags: ["form"],
      component: Hero,
      states: { Wide: { props: { destination: "Lisbon" } } },
    } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
            ),
        ],
        &[
            (
                "a-string",
                r#"export default {
  id: "trip-planner",
  tags: "form",
  component: TripPlanner,
  states: { Default: {} },
} satisfies Previews<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type 'readonly string[]'"],
            ),
            (
                "a-number-in-the-list",
                r#"export default {
  id: "trip-planner",
  tags: ["form", 2],
  component: TripPlanner,
  states: { Default: {} },
} satisfies Previews<typeof TripPlanner>;"#,
                &["Type 'number' is not assignable to type 'string'"],
            ),
            (
                "an-object",
                r#"export default {
  id: "trip-planner",
  tags: { form: true },
  component: TripPlanner,
  states: { Default: {} },
} satisfies Previews<typeof TripPlanner>;"#,
                &["'form' does not exist in type 'readonly string[]'"],
            ),
            (
                "a-private-string",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    {
      id: "hero",
      tags: "form",
      component: Hero,
      states: { Wide: { props: { destination: "Lisbon" } } },
    } satisfies PrivatePreviews<typeof Hero>,
  ],
} satisfies Previews<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type 'readonly string[]'"],
            ),
            (
                "of-a-state",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { tags: ["form"] } },
} satisfies Previews<typeof TripPlanner>;"#,
                &["'tags' does not exist in type"],
            ),
        ],
    );
}
