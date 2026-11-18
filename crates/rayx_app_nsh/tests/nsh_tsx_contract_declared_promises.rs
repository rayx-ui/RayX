//! An app contract's command declares its result as a promise, its own `#[ts("…")]` included
//! (NSH declares a `#[ts]` as written), so a TS implementation's `async` methods type-check
//! against the interface; a posted command stays `void`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod headless;

use rayx_app_nsh::ts::ContractRegistryExt as _;

/// A catalog written in TS.
#[rayx_app_nsh::contract]
pub trait Catalog {
    /// The title of `id`, typed by its own signature.
    #[ts("(id: string): string | null")]
    async fn title(&self, id: String) -> Result<Option<String>, String>;
    /// How many entries there are.
    async fn count(&self) -> Result<u32, String>;
    /// Notes `line`.
    fn note(&self, line: String);
}

#[test]
fn commands_are_declared_as_promises() {
    let ts = headless::declarations(&headless::set(|set| {
        set.contract::<dyn Catalog>();
    }));
    for declared in [
        "  title(id: string): Promise<string | null>;",
        "  count(): Promise<number>;",
        "  note(line: string): void;",
    ] {
        assert!(ts.contains(declared), "no `{declared}` in\n{ts}");
    }
}
