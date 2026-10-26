//! Configuration: sources merged in order, read by path and section, bound onto values, set and
//! reloaded with change notifications.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::executor::block_on;
use rayx_core::configuration::{
    ConfigurationBuilder, ConfigurationError, ConfigurationSource, SourceLoad, Value,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Retry {
    limit: u32,
    delay_ms: u64,
}

/// A source that loads a counter's next value each time, now or asynchronously.
struct CountingSource {
    loads: Arc<AtomicU32>,
    pending: bool,
}

impl ConfigurationSource for CountingSource {
    fn name(&self) -> String {
        "counter".to_owned()
    }

    fn load(&self) -> SourceLoad {
        let loads = Arc::clone(&self.loads);
        let load = move || Ok(json!({ "Loads": loads.fetch_add(1, Ordering::SeqCst) + 1 }));
        if self.pending {
            SourceLoad::Pending(Box::pin(async move { load() }))
        } else {
            SourceLoad::Ready(load())
        }
    }
}

#[test]
fn di_configuration_merges_sources_in_order() -> TestResult {
    let mut builder = ConfigurationBuilder::new();
    builder
        .add_value(json!({ "Feed": { "Label": "value", "Hosts": ["a", "b", "c"], "Keep": true } }))
        .add_json(r#"{ "feed": { "label": "json", "hosts": ["d"] }, "Theme": "dark" }"#)
        .add_in_memory([
            ("Feed:Retry:limit", json!(3)),
            ("Feed:retry:delay_ms", json!(250)),
        ]);
    let configuration = builder.build()?;

    assert_eq!(
        configuration.get::<String>("FEED:LABEL")?.as_deref(),
        Some("json")
    );
    assert_eq!(
        configuration.get::<Vec<String>>("Feed:Hosts")?,
        Some(vec!["d".to_owned()])
    );
    assert_eq!(configuration.get::<bool>("Feed:Keep")?, Some(true));
    assert_eq!(
        configuration.get::<Retry>("Feed:Retry")?,
        Some(Retry {
            limit: 3,
            delay_ms: 250
        })
    );
    assert_eq!(configuration.get::<String>("Feed:Missing")?, None);
    assert_eq!(configuration.value("Feed:Hosts:0"), Some(json!("d")));
    assert!(matches!(
        configuration.get::<u32>("Theme"),
        Err(ConfigurationError::Get { path, .. }) if path == "Theme"
    ));
    Ok(())
}

#[test]
fn di_configuration_sections_read_and_bind() -> TestResult {
    let mut builder = ConfigurationBuilder::new();
    builder.add_value(json!({
        "Feed": { "Retry": { "limit": 5 }, "Hosts": ["a", "b"] },
        "Empty": null
    }));
    let configuration = builder.build()?;

    let feed = configuration.section("Feed");
    assert!(feed.exists());
    assert_eq!((feed.path(), feed.key()), ("Feed", "Feed"));
    let retry = feed.section("retry");
    assert_eq!((retry.path(), retry.key()), ("Feed:retry", "retry"));
    assert_eq!(retry.section("Limit").get::<u32>()?, Some(5));
    assert!(!configuration.section("Empty").exists());
    assert!(!configuration.section("Feed:Missing").exists());

    let children = feed.children();
    let keys = children.iter().map(|child| child.key()).collect::<Vec<_>>();
    assert_eq!(keys, ["Hosts", "Retry"]);
    let hosts = feed
        .section("Hosts")
        .children()
        .iter()
        .map(|host| host.get::<String>().unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(hosts, ["a", "b"]);
    assert_eq!(configuration.children().len(), 2);

    let mut bound = Retry {
        limit: 1,
        delay_ms: 100,
    };
    retry.bind(&mut bound)?;
    assert_eq!(
        bound,
        Retry {
            limit: 5,
            delay_ms: 100
        }
    );
    configuration.bind("Missing", &mut bound)?;
    assert_eq!(bound.limit, 5);
    Ok(())
}

#[test]
fn di_configuration_set_and_reload_notify_listeners() -> TestResult {
    let loads = Arc::new(AtomicU32::new(0));
    let mut builder = ConfigurationBuilder::new();
    builder.add_source(CountingSource {
        loads: Arc::clone(&loads),
        pending: false,
    });
    let configuration = builder.build()?;
    let changes = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&changes);
    let subscription = configuration.on_change(move || *counted.lock().unwrap() += 1);

    assert_eq!(configuration.get::<u32>("Loads")?, Some(1));
    configuration.set("Feed:Label", "set");
    configuration.reload()?;
    assert_eq!(configuration.get::<u32>("Loads")?, Some(2));
    assert_eq!(
        configuration.get::<String>("Feed:Label")?.as_deref(),
        Some("set"),
        "a set value outlives reloads"
    );
    configuration.set("Loads", json!(10));
    assert_eq!(configuration.get::<u32>("loads")?, Some(10));
    assert_eq!(*changes.lock().unwrap(), 3);

    drop(subscription);
    configuration.reload()?;
    assert_eq!(*changes.lock().unwrap(), 3);
    assert_eq!(loads.load(Ordering::SeqCst), 3);
    Ok(())
}

#[test]
fn di_configuration_async_sources_need_async_builds() -> TestResult {
    let source = || CountingSource {
        loads: Arc::new(AtomicU32::new(0)),
        pending: true,
    };
    let mut sync = ConfigurationBuilder::new();
    sync.add_source(source());
    assert_eq!(
        sync.build().unwrap_err(),
        ConfigurationError::AsyncSource {
            source_name: "counter".to_owned()
        }
    );

    let mut builder = ConfigurationBuilder::new();
    builder.add_source(source());
    let configuration = block_on(builder.build_async())?;
    assert_eq!(configuration.get::<u32>("Loads")?, Some(1));
    assert!(configuration.reload().is_err());
    block_on(configuration.reload_async())?;
    assert_eq!(configuration.get::<u32>("Loads")?, Some(2));
    Ok(())
}

#[test]
fn di_configuration_sources_load_objects() {
    let mut json = ConfigurationBuilder::new();
    json.add_json("{ not json");
    let error = json.build().unwrap_err();
    assert!(
        matches!(&error, ConfigurationError::Source { source_name, .. } if source_name == "JSON")
    );
    assert!(error.to_string().contains("JSON"), "{error}");

    let mut array = ConfigurationBuilder::new();
    array.add_value(json!([1, 2]));
    assert_eq!(
        array.build().unwrap_err(),
        ConfigurationError::NotAnObject {
            source_name: "in-memory values".to_owned(),
            kind: "an array"
        }
    );

    let mut nothing = ConfigurationBuilder::new();
    nothing.add_value(Value::Null).add_json("null");
    let configuration = nothing.build().unwrap();
    assert_eq!(*configuration.root(), json!({}));
}
