//! A preview state's `services`: a function a design host runs with the instance's own scope's
//! registrations (`MetaServices`, typed by their shape), registering singletons, scoped and
//! transient services and their `As` forms under object tokens, chained or not; a state without
//! `services` type checks as before, and a private component's states take them too. What is no
//! function, a method the registrations have not (a startup one) and a token that is no object
//! are refused. The type checks run a TypeScript compiler (`support::typecheck`), so they are
//! `#[ignore]`d: `cargo test -p rayx_components --test meta_services -- --include-ignored`.

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
fn a_state_takes_optional_services_over_the_scope_s_registrations() {
    assert!(declared("MetaState<P>").contains("readonly services?: (scope: MetaServices) => void"));
    let registrations = declared("MetaServices");
    for method in [
        "addSingleton(",
        "addSingletonAs(",
        "addScoped(",
        "addScopedAs(",
        "addTransient(",
        "addTransientAs(",
    ] {
        assert!(
            registrations.contains(&format!(" {method}token: object, factory: (services: ")),
            "no `{method}` in {registrations}"
        );
    }
    assert!(!registrations.contains("Startup"), "{registrations}");
    assert_eq!(rayx_components::set().undocumented(), Vec::<String>::new());
}

/// What the cases' services register: a view model's contract, its design implementation and
/// a token for a value.
const TOKENS: &str = r#"
interface TripViewModel { readonly days: number }
class DesignTripViewModel implements TripViewModel { constructor(readonly days: number) {} }
abstract class TripViewModelToken {}
class Clock { now() { return 7; } }
"#;

#[test]
#[ignore = "needs a TypeScript compiler: NSH_TSC, or the one `rayx app check` keeps in NSH's cache"]
fn services_register_in_the_instance_s_scope() {
    let case = |text: &str| format!("{TOKENS}\n{text}");
    let accepted = [
        (
            "every-form",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: {
    Long: {
      props: { traveler: "Ana" },
      services: (scope) => {
        scope.addSingleton(Clock, () => new Clock());
        scope.addSingletonAs(TripViewModelToken, (services) =>
          new DesignTripViewModel(services.getRequiredService(Clock).now()));
        scope.addScoped(Clock, () => new Clock());
        scope.addScopedAs(TripViewModelToken, () => new DesignTripViewModel(2));
        scope.addTransient(Clock, (services) => services.getService(Clock) ?? new Clock());
        scope.addTransientAs(TripViewModelToken, () => new DesignTripViewModel(3));
      },
    },
  },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ),
        (
            "chained",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: {
    Short: {
      services: (scope) => {
        scope
          .addSingleton(Clock, () => new Clock())
          .addSingletonAs(TripViewModelToken, () => new DesignTripViewModel(1));
      },
    },
  },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ),
        (
            "without",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { props: { traveler: "Ana" } } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ),
        (
            "of-a-private-component",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: {} },
  private: [
    {
      id: "hero",
      component: Hero,
      states: {
        Wide: {
          props: { destination: "Lisbon" },
          services: (scope) => { scope.addSingleton(Clock, () => new Clock()); },
        },
      },
    } satisfies PrivateMeta<typeof Hero>,
  ],
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
        ),
    ];
    let refused = [
        (
            "not-a-function",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: { Default: { services: "clock" } },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            &["Type 'string' is not assignable to type '(scope: MetaServices) => void'"][..],
        ),
        (
            "a-startup-service",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: {
    Default: {
      services: (scope) => { scope.addStartupSingleton(Clock, () => new Clock()); },
    },
  },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            &["Property 'addStartupSingleton' does not exist on type 'MetaServices'"][..],
        ),
        (
            "a-token-that-is-no-object",
            case(
                r#"export default {
  id: "trip-planner",
  component: TripPlanner,
  states: {
    Default: {
      services: (scope) => { scope.addSingleton("clock", () => new Clock()); },
    },
  },
} satisfies ComponentMeta<typeof TripPlanner>;"#,
            ),
            &["Argument of type 'string' is not assignable to parameter of type 'object'"][..],
        ),
    ];
    let accepted: Vec<(&str, &str)> = accepted
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let refused: Vec<(&str, &str, &[&str])> = refused
        .iter()
        .map(|(name, text, said)| (*name, text.as_str(), *said))
        .collect();
    typecheck::check("services", &accepted, &refused);
}
