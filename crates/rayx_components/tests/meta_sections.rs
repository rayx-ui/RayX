//! A component's meta in sections: `ComponentMeta<C>` and `PrivateMeta<C>` take a Markdown
//! `description` and either named `states` or `sections`, never both; a section
//! (`MetaSection<P>`) is a `title`, a `description` and its own named `states`, whose props are
//! checked against the component as the top level's are. The type checks run a TypeScript
//! compiler (`support::typecheck`), so they are `#[ignore]`d:
//! `cargo test -p rayx_components --test meta_sections -- --include-ignored`.

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
fn the_set_declares_sections_and_the_either_or() {
    assert_eq!(
        declared("MetaSection<P>"),
        "export type MetaSection<P> = { readonly title: string; readonly description?: string; \
         readonly states: { readonly [name: string]: MetaState<P> } };"
    );
    for name in ["ComponentMeta<C>", "PrivateMeta<C>"] {
        let alias = declared(name);
        for member in [
            "readonly description?: string;",
            "readonly sections?: never }",
            "readonly states?: never }",
            "{ readonly sections: readonly MetaSection<C extends (props: infer P) => unknown ? P : never>[];",
        ] {
            assert!(alias.contains(member), "no `{member}` in {alias}");
        }
    }
}

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn sections_take_the_place_of_states() {
    typecheck::check(
        "sections",
        &[
            (
                "states",
                r#"export default {
  id: "trip-planner",
  description: "A trip's plan, day by day.",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            (
                "sections",
                r#"export default {
  id: "trip-planner",
  title: "Trip planner",
  description: `A trip's **plan**, day by day.`,
  component: TripPlanner,
  sections: [
    {
      title: "Lengths",
      description: "Short and long trips.",
      states: {
        Weekend: { props: { traveler: "Ana", days: 2 } },
        Fortnight: { props: { traveler: "Ana", days: 14 } },
      },
    },
    { title: "Travelers", states: { Solo: { props: { traveler: "Ivo" } } } },
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            (
                "private-sections",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  sections: [{ title: "Lengths", states: { Weekend: { props: { traveler: "Ana", days: 2 } } } }],
  private: [
    {
      id: "hero",
      description: "The picture at the top.",
      component: Hero,
      sections: [
        { title: "Widths", description: "Narrow and wide.", states: { Wide: { props: { destination: "Lisbon", wide: true } } } },
      ],
    } satisfies PrivateMeta<typeof Hero>,
    { id: "stat", component: Stat, states: { Days: { props: { value: "3", label: "days" } } } } satisfies PrivateMeta<typeof Stat>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ],
        &[
            (
                "both",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
  sections: [{ title: "Lengths", states: { Weekend: { props: { traveler: "Ana", days: 2 } } } }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Types of property"],
            ),
            (
                "untitled-section",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  sections: [{ description: "No title.", states: { Weekend: { props: { traveler: "Ana" } } } }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["title"],
            ),
            (
                "wrong-prop-type",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  sections: [{ title: "Lengths", states: { Weekend: { props: { traveler: "Ana", days: "two" } } } }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'string' is not assignable to type 'number'"],
            ),
            (
                "unknown-prop",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  sections: [{ title: "Lengths", states: { Weekend: { props: { traveler: "Ana", nights: 2 } } } }],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["nights"],
            ),
            (
                "private-both",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
  private: [
    {
      id: "hero",
      component: Hero,
      states: { Wide: { props: { destination: "Lisbon" } } },
      sections: [{ title: "Widths", states: { Narrow: { props: { destination: "Lisbon" } } } }],
    } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Types of property"],
            ),
            (
                "private-wrong-prop",
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
  private: [
    {
      id: "hero",
      component: Hero,
      sections: [{ title: "Widths", states: { Wide: { props: { destination: 7 } } } }],
    } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
                &["Type 'number' is not assignable to type 'string'"],
            ),
        ],
    );
}
