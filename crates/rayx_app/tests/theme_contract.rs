//! The theme contract in the core, NSH-free: its data types cross to TS as JSON, the fake
//! (`rayx_app::testing::FakeTheme`, registered as `dyn ThemeService` in a plain provider)
//! publishes the snapshot a test gives it and records the commands, refusing unknown modifiers
//! and contexts, and a failed call is in the contract's error type.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::services::ContractFailure;
use rayx_app::services::theme::{
    ColorModeChoice, ThemeError, ThemeModifier, ThemeService, ThemeSnapshot,
};
use rayx_app::testing::{FakeTheme, ThemeCall};
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use serde_json::json;

fn modifier(name: &str, contexts: &[&str], selected: &str) -> ThemeModifier {
    ThemeModifier {
        name: name.into(),
        contexts: contexts.iter().map(|context| (*context).into()).collect(),
        default: contexts[0].into(),
        selected: selected.into(),
    }
}

fn snapshot() -> ThemeSnapshot {
    ThemeSnapshot {
        modifiers: vec![
            modifier("colorMode", &["light", "dark"], "light"),
            modifier("accent", &["blue", "cyan", "sky"], "blue"),
        ],
        color_mode: ColorModeChoice::System,
        revision: 3,
    }
}

#[test]
fn the_data_types_cross_as_json() {
    let value = serde_json::to_value(snapshot()).unwrap();
    assert_eq!(
        value,
        json!({
            "modifiers": [
                { "name": "colorMode", "contexts": ["light", "dark"], "default": "light", "selected": "light" },
                { "name": "accent", "contexts": ["blue", "cyan", "sky"], "default": "blue", "selected": "blue" }
            ],
            "color_mode": "System",
            "revision": 3
        })
    );
    let back: ThemeSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(back, snapshot());

    for (choice, text) in [
        (ColorModeChoice::System, "System"),
        (ColorModeChoice::Light, "Light"),
        (ColorModeChoice::Dark, "Dark"),
    ] {
        assert_eq!(serde_json::to_value(choice).unwrap(), json!(text));
        assert_eq!(
            serde_json::from_value::<ColorModeChoice>(json!(text)).unwrap(),
            choice
        );
    }
    assert!(serde_json::from_value::<ColorModeChoice>(json!("Auto")).is_err());

    // A TS implementation may leave fields out: they are their defaults.
    let partial: ThemeSnapshot =
        serde_json::from_value(json!({ "modifiers": [{ "name": "density" }] })).unwrap();
    assert_eq!(partial.color_mode, ColorModeChoice::System);
    assert_eq!(partial.revision, 0);
    assert_eq!(partial.modifiers[0].name, "density");
    assert!(partial.modifiers[0].contexts.is_empty());
}

#[test]
fn the_snapshot_answers_by_modifier() {
    let snapshot = snapshot();
    assert_eq!(snapshot.selected("accent"), Some("blue"));
    assert_eq!(snapshot.selected("density"), None);
    assert!(snapshot.modifier("accent").unwrap().has_context("cyan"));
    assert!(!snapshot.modifier("accent").unwrap().has_context("red"));
}

#[test]
fn the_fake_is_a_theme_resolved_by_trait() {
    let fake = FakeTheme::new(snapshot());
    let mut builder = RayXAppBuilder::new();
    builder
        .services_mut()
        .add_singleton_as::<dyn ThemeService>(Arc::clone(&fake) as Arc<dyn ThemeService>);
    let services = builder.build().unwrap();
    let theme = services.required_service::<dyn ThemeService>().unwrap();

    let heard = Arc::new(Mutex::new(Vec::new()));
    let heard_by = Arc::clone(&heard);
    let _subscription = theme.theme().subscribe(move |snapshot| {
        heard_by.lock().unwrap().push((
            snapshot.selected("accent").unwrap().to_owned(),
            snapshot.revision,
        ));
    });

    assert!(block_on(theme.set_context("accent".into(), "cyan".into())).unwrap());
    assert!(!block_on(theme.set_context("accent".into(), "cyan".into())).unwrap());
    assert_eq!(theme.selected("accent").as_deref(), Some("cyan"));
    assert_eq!(theme.snapshot().revision, 4);

    assert!(block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    assert!(!block_on(theme.set_color_mode(ColorModeChoice::Dark)).unwrap());
    assert_eq!(theme.color_mode(), ColorModeChoice::Dark);
    assert_eq!(theme.snapshot().revision, 5);

    fake.publish(ThemeSnapshot {
        revision: 9,
        ..snapshot()
    });
    assert_eq!(theme.selected("accent").as_deref(), Some("blue"));
    assert_eq!(
        theme.modifier("colorMode").unwrap().contexts,
        ["light", "dark"]
    );

    assert_eq!(
        fake.calls(),
        [
            ThemeCall::SetContext {
                modifier: "accent".into(),
                context: "cyan".into()
            },
            ThemeCall::SetContext {
                modifier: "accent".into(),
                context: "cyan".into()
            },
            ThemeCall::SetColorMode(ColorModeChoice::Dark),
            ThemeCall::SetColorMode(ColorModeChoice::Dark),
        ]
    );
    assert_eq!(
        *heard.lock().unwrap(),
        [
            ("cyan".to_owned(), 4),
            ("cyan".to_owned(), 5),
            ("blue".to_owned(), 9)
        ]
    );
}

#[test]
fn unknown_modifiers_and_contexts_are_refused_and_change_nothing() {
    let fake = FakeTheme::new(snapshot());
    let theme = Arc::clone(&fake) as Arc<dyn ThemeService>;

    assert_eq!(
        block_on(theme.set_context("brand".into(), "forest".into())),
        Err(ThemeError::UnknownModifier {
            modifier: "brand".into()
        })
    );
    assert_eq!(
        block_on(theme.set_context("accent".into(), "red".into())),
        Err(ThemeError::UnknownContext {
            modifier: "accent".into(),
            context: "red".into()
        })
    );
    assert_eq!(theme.snapshot(), snapshot());
    assert_eq!(fake.calls().len(), 2);

    let unknown = ThemeError::UnknownModifier {
        modifier: "brand".into(),
    };
    assert_eq!(unknown.to_string(), "the theme has no modifier `brand`");
    let context = ThemeError::UnknownContext {
        modifier: "accent".into(),
        context: "red".into(),
    };
    assert_eq!(
        context.to_string(),
        "the theme's modifier `accent` has no context `red`"
    );
}

#[test]
fn a_failed_call_is_the_contract_s_error() {
    let failure = ContractFailure {
        contract: "ThemeServiceContract",
        member: "setContext",
        message: "the engine is gone".into(),
    };
    let error = ThemeError::from(failure.clone());
    assert_eq!(error, ThemeError::Failed(failure));
    assert_eq!(
        error.to_string(),
        "`ThemeServiceContract.setContext`: the engine is gone"
    );
    let source: &dyn std::error::Error = &error;
    assert!(source.to_string().contains("the engine is gone"));
}
