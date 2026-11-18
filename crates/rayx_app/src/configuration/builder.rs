use super::source::{JsonSource, MemorySource};
use super::{Configuration, ConfigurationError, ConfigurationSource};
use serde_json::Value;

/// Collects configuration sources (.NET's `ConfigurationBuilder`) and builds them into a
/// [`Configuration`]. Later sources' values go over earlier ones'.
#[derive(Default)]
pub struct ConfigurationBuilder {
    sources: Vec<Box<dyn ConfigurationSource>>,
}

impl ConfigurationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a source.
    pub fn add_source(&mut self, source: impl ConfigurationSource) -> &mut Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Adds values by path (.NET's `AddInMemoryCollection`):
    /// `[("Feed:Label", json!("live")), ("Feed:Retry:Limit", json!(3))]`.
    pub fn add_in_memory<K, V>(&mut self, values: impl IntoIterator<Item = (K, V)>) -> &mut Self
    where
        K: Into<String>,
        V: Into<Value>,
    {
        self.add_source(MemorySource::from_pairs(
            values
                .into_iter()
                .map(|(path, value)| (path.into(), value.into())),
        ))
    }

    /// Adds a tree of values (an object), such as `json!({ "Feed": { "Label": "live" } })`.
    pub fn add_value(&mut self, values: Value) -> &mut Self {
        self.add_source(MemorySource { values })
    }

    /// Adds JSON text (an object).
    pub fn add_json(&mut self, json: impl Into<String>) -> &mut Self {
        self.add_source(JsonSource { json: json.into() })
    }

    /// How many sources there are.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Loads every source and merges them. A source that loads asynchronously fails it
    /// ([`ConfigurationError::AsyncSource`]); use [`Self::build_async`] then.
    pub fn build(self) -> Result<Configuration, ConfigurationError> {
        Configuration::load(self.sources)
    }

    /// [`Self::build`], awaiting asynchronous sources one after another, in order.
    pub async fn build_async(self) -> Result<Configuration, ConfigurationError> {
        Configuration::load_async(self.sources).await
    }
}

impl std::fmt::Debug for ConfigurationBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigurationBuilder")
            .field("sources", &self.sources.len())
            .finish()
    }
}
