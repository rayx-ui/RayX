//! The configuration builder `configureAppConfiguration` hands its callback.

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::configuration::ConfigurationBuilder as Configuration;

use crate::configuration::options_ts::push;

/// The app's configuration sources (`configureAppConfiguration`'s callback), as Rust's
/// `ConfigurationBuilder` adds them: each method adds one, after those before it, and returns
/// the object, so calls chain.
#[class]
pub struct ConfigurationBuilder {
    steps: Vec<ConfigurationStep>,
}

/// What one `ConfigurationBuilder` method added.
#[derive(Clone, Debug)]
pub(crate) enum ConfigurationStep {
    Json(String),
    InMemory(Vec<(String, JsonValue)>),
    Value(JsonValue),
}

impl ConfigurationStep {
    pub(crate) fn apply(self, configuration: &mut Configuration) {
        match self {
            Self::Json(json) => configuration.add_json(json),
            Self::InMemory(values) => configuration.add_in_memory(values),
            Self::Value(value) => configuration.add_value(value),
        };
    }
}

impl ConfigurationBuilder {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<ConfigurationStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl ConfigurationBuilder {
    /// Adds JSON text (an object) as a source.
    pub fn add_json(this: Instance<Self>, json: String) -> Instance<Self> {
        push(this, |o| o.steps.push(ConfigurationStep::Json(json)))
    }

    /// Adds values by path (.NET's `AddInMemoryCollection`):
    /// `{ "Feed:Label": "live", "Feed:Retry:Limit": 3 }`.
    pub fn add_in_memory(
        this: Instance<Self>,
        values: JsonValue,
    ) -> Result<Instance<Self>, String> {
        let JsonValue::Object(values) = values else {
            return Err("addInMemory takes an object of values by path".into());
        };
        let values = values.into_iter().collect();
        Ok(push(this, |o| {
            o.steps.push(ConfigurationStep::InMemory(values))
        }))
    }

    /// Adds a tree of values (an object) as a source: `{ Feed: { Label: "live" } }`.
    pub fn add_value(this: Instance<Self>, value: JsonValue) -> Instance<Self> {
        push(this, |o| o.steps.push(ConfigurationStep::Value(value)))
    }
}
