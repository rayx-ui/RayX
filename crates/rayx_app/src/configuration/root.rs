use super::source::{ConfigurationSource, SourceLoad};
use super::{ConfigurationError, tree};
use crate::change::{ChangeSubscription, Listeners};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::any::type_name;
use std::sync::{Arc, PoisonError, RwLock};

/// The app's configuration (.NET's `IConfiguration`): the merged tree of its sources' values,
/// read by path (`"Feed:Retry:Limit"`; keys match ignoring ASCII case, `_` and `-`).
///
/// Built by [`ConfigurationBuilder`](super::ConfigurationBuilder); the app builder registers it
/// as a singleton, which is where [`OptionsBuilder::bind`](crate::options::OptionsBuilder::bind)
/// finds it. A clone is the same configuration. [`Self::reload`] and [`Self::set`] change it and
/// notify [`Self::on_change`]'s listeners, and through them every [`OptionsMonitor`] of options
/// bound to it.
///
/// [`OptionsMonitor`]: crate::options::OptionsMonitor
#[derive(Clone)]
pub struct Configuration {
    inner: Arc<Inner>,
}

struct Inner {
    sources: Vec<Box<dyn ConfigurationSource>>,
    state: RwLock<State>,
    listeners: Listeners<dyn Fn() + Send + Sync>,
}

struct State {
    /// Each source's tree, in order.
    layers: Vec<Value>,
    /// What [`Configuration::set`] wrote, over every source.
    overrides: Value,
    merged: Arc<Value>,
}

impl State {
    fn remerge(&mut self) {
        let mut merged = Value::Object(Map::new());
        for layer in self.layers.iter().chain([&self.overrides]) {
            if !layer.is_null() {
                tree::merge(&mut merged, layer);
            }
        }
        self.merged = Arc::new(merged);
    }
}

impl Configuration {
    pub(super) fn load(
        sources: Vec<Box<dyn ConfigurationSource>>,
    ) -> Result<Self, ConfigurationError> {
        let layers = sources
            .iter()
            .map(|source| ready(source.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::with_layers(sources, layers))
    }

    pub(super) async fn load_async(
        sources: Vec<Box<dyn ConfigurationSource>>,
    ) -> Result<Self, ConfigurationError> {
        let mut layers = Vec::with_capacity(sources.len());
        for source in &sources {
            layers.push(loaded(source.as_ref()).await?);
        }
        Ok(Self::with_layers(sources, layers))
    }

    fn with_layers(sources: Vec<Box<dyn ConfigurationSource>>, layers: Vec<Value>) -> Self {
        let mut state = State {
            layers,
            overrides: Value::Null,
            merged: Arc::default(),
        };
        state.remerge();
        Self {
            inner: Arc::new(Inner {
                sources,
                state: RwLock::new(state),
                listeners: Listeners::default(),
            }),
        }
    }

    /// The whole merged tree now.
    pub fn root(&self) -> Arc<Value> {
        let state = self
            .inner
            .state
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(&state.merged)
    }

    /// The value at `path` (a scalar, or a section's tree), if any.
    pub fn value(&self, path: &str) -> Option<Value> {
        tree::lookup(&self.root(), path).cloned()
    }

    /// The value at `path` as a `T`; `None` when there is none (or it is `null`). It converts
    /// with serde as it is, so a struct's keys must be its serde names; [`Self::bind`] matches
    /// them loosely.
    pub fn get<T>(&self, path: &str) -> Result<Option<T>, ConfigurationError>
    where
        T: DeserializeOwned,
    {
        match self.value(path) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => {
                serde_json::from_value(value)
                    .map(Some)
                    .map_err(|error| ConfigurationError::Get {
                        path: tree::canonical(path),
                        type_name: type_name::<T>(),
                        message: error.to_string(),
                    })
            }
        }
    }

    /// The section at `path`. It exists or not ([`ConfigurationSection::exists`]); it reads the
    /// configuration as it is when read.
    pub fn section(&self, path: &str) -> ConfigurationSection {
        ConfigurationSection {
            configuration: self.clone(),
            path: tree::canonical(path),
        }
    }

    /// The top-level sections, sorted by key.
    pub fn children(&self) -> Vec<ConfigurationSection> {
        self.section("").children()
    }

    /// Binds the section at `path` onto `target` (.NET's `Bind`): the section's values replace
    /// `target`'s, key by key, and what the section does not name keeps its value. Keys match
    /// ignoring ASCII case, `_` and `-`, so a `connectionString` key binds a `connection_string`
    /// field. A list is replaced as a whole (.NET appends to it), and a `null` binds nothing.
    /// Without the section, `target` is unchanged.
    pub fn bind<T>(&self, path: &str, target: &mut T) -> Result<(), ConfigurationError>
    where
        T: Serialize + DeserializeOwned,
    {
        let root = self.root();
        let Some(section) = tree::lookup(&root, path) else {
            return Ok(());
        };
        let failed = |message: String| ConfigurationError::Bind {
            path: tree::canonical(path),
            type_name: type_name::<T>(),
            message,
        };
        let mut value =
            serde_json::to_value(&*target).map_err(|error| failed(error.to_string()))?;
        tree::overlay(&mut value, section);
        *target = serde_json::from_value(value).map_err(|error| failed(error.to_string()))?;
        Ok(())
    }

    /// Sets the value at `path` over every source's (it outlives reloads), then notifies the
    /// listeners.
    pub fn set(&self, path: &str, value: impl Into<Value>) {
        {
            let mut state = self
                .inner
                .state
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            tree::set(&mut state.overrides, path, value.into());
            state.remerge();
        }
        self.notify();
    }

    /// Loads every source again (.NET's `Reload`) and notifies the listeners. On a failure the
    /// configuration stays as it was.
    pub fn reload(&self) -> Result<(), ConfigurationError> {
        let layers = self
            .inner
            .sources
            .iter()
            .map(|source| ready(source.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        self.replace_layers(layers);
        Ok(())
    }

    /// [`Self::reload`] with asynchronous sources.
    pub async fn reload_async(&self) -> Result<(), ConfigurationError> {
        let mut layers = Vec::with_capacity(self.inner.sources.len());
        for source in &self.inner.sources {
            layers.push(loaded(source.as_ref()).await?);
        }
        self.replace_layers(layers);
        Ok(())
    }

    /// Calls `listener` after every change ([`Self::set`], [`Self::reload`]) until the returned
    /// subscription is dropped.
    pub fn on_change(&self, listener: impl Fn() + Send + Sync + 'static) -> ChangeSubscription {
        self.inner.listeners.add(Arc::new(listener))
    }

    fn replace_layers(&self, layers: Vec<Value>) {
        {
            let mut state = self
                .inner
                .state
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            state.layers = layers;
            state.remerge();
        }
        self.notify();
    }

    fn notify(&self) {
        for listener in self.inner.listeners.current() {
            listener();
        }
    }
}

/// An empty configuration, without sources.
impl Default for Configuration {
    fn default() -> Self {
        Self::with_layers(Vec::new(), Vec::new())
    }
}

impl std::fmt::Debug for Configuration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Configuration")
            .field("sources", &self.inner.sources.len())
            .field("listeners", &self.inner.listeners.len())
            .finish()
    }
}

/// A section of the configuration (.NET's `IConfigurationSection`): a path into it.
#[derive(Clone, Debug)]
pub struct ConfigurationSection {
    configuration: Configuration,
    path: String,
}

impl ConfigurationSection {
    /// The section's full path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The last segment of the path.
    pub fn key(&self) -> &str {
        tree::segments(&self.path).last().unwrap_or("")
    }

    /// Whether the section has a value (`null` counts as none).
    pub fn exists(&self) -> bool {
        self.configuration
            .value(&self.path)
            .is_some_and(|value| !value.is_null())
    }

    /// The section's value (a scalar, or its tree).
    pub fn value(&self) -> Option<Value> {
        self.configuration.value(&self.path)
    }

    /// The section as a `T` ([`Configuration::get`]).
    pub fn get<T>(&self) -> Result<Option<T>, ConfigurationError>
    where
        T: DeserializeOwned,
    {
        self.configuration.get(&self.path)
    }

    /// A section within this one.
    pub fn section(&self, path: &str) -> ConfigurationSection {
        self.configuration.section(&tree::join(&self.path, path))
    }

    /// The sections within this one: an object's keys (sorted, ignoring ASCII case), an array's
    /// indexes.
    pub fn children(&self) -> Vec<ConfigurationSection> {
        let keys = match self.value() {
            Some(Value::Object(object)) => {
                let mut keys = object.keys().cloned().collect::<Vec<_>>();
                // Sorted as .NET sorts them, whatever order the tree keeps.
                keys.sort_by_key(|key| key.to_ascii_lowercase());
                keys
            }
            Some(Value::Array(items)) => (0..items.len()).map(|index| index.to_string()).collect(),
            _ => Vec::new(),
        };
        keys.iter().map(|key| self.section(key)).collect()
    }

    /// Binds the section onto `target` ([`Configuration::bind`]).
    pub fn bind<T>(&self, target: &mut T) -> Result<(), ConfigurationError>
    where
        T: Serialize + DeserializeOwned,
    {
        self.configuration.bind(&self.path, target)
    }
}

fn checked(
    source: &dyn ConfigurationSource,
    result: Result<Value, String>,
) -> Result<Value, ConfigurationError> {
    match result {
        Ok(value @ (Value::Object(_) | Value::Null)) => Ok(value),
        Ok(value) => Err(ConfigurationError::NotAnObject {
            source_name: source.name(),
            kind: tree::kind(&value),
        }),
        Err(message) => Err(ConfigurationError::Source {
            source_name: source.name(),
            message,
        }),
    }
}

fn ready(source: &dyn ConfigurationSource) -> Result<Value, ConfigurationError> {
    match source.load() {
        SourceLoad::Ready(result) => checked(source, result),
        SourceLoad::Pending(_) => Err(ConfigurationError::AsyncSource {
            source_name: source.name(),
        }),
    }
}

async fn loaded(source: &dyn ConfigurationSource) -> Result<Value, ConfigurationError> {
    match source.load() {
        SourceLoad::Ready(result) => checked(source, result),
        SourceLoad::Pending(loading) => checked(source, loading.await),
    }
}
