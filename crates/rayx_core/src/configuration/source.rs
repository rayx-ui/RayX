use super::tree;
use futures::future::LocalBoxFuture;
use serde_json::{Map, Value};

/// Where configuration comes from: a file, the app's bundled settings, a server. A source loads
/// a tree of values (an object, or `null` for nothing); the configuration merges the sources'
/// trees in the order they were added, a later one's values over an earlier one's.
///
/// A source is loaded when the configuration is built and again on every reload, so it reads
/// its data each time it loads rather than once.
pub trait ConfigurationSource: Send + Sync + 'static {
    /// The source's name in errors, such as `appsettings.json`.
    fn name(&self) -> String;

    /// Loads the source's tree, now or asynchronously.
    fn load(&self) -> SourceLoad;
}

/// What [`ConfigurationSource::load`] gives: the tree, or a future of it. A failure is a message
/// that the configuration's error names the source in.
pub enum SourceLoad {
    Ready(Result<Value, String>),
    /// Loads asynchronously: such a source needs an async build or reload.
    Pending(LocalBoxFuture<'static, Result<Value, String>>),
}

impl std::fmt::Debug for SourceLoad {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(result) => formatter.debug_tuple("Ready").field(result).finish(),
            Self::Pending(_) => formatter.write_str("Pending"),
        }
    }
}

/// Values held in memory (.NET's in-memory collection): keys are paths (`"Feed:Label"`).
pub(crate) struct MemorySource {
    pub(crate) values: Value,
}

impl MemorySource {
    pub(crate) fn from_pairs(pairs: impl IntoIterator<Item = (String, Value)>) -> Self {
        let mut values = Value::Object(Map::new());
        for (path, value) in pairs {
            tree::set(&mut values, &path, value);
        }
        Self { values }
    }
}

impl ConfigurationSource for MemorySource {
    fn name(&self) -> String {
        "in-memory values".to_owned()
    }

    fn load(&self) -> SourceLoad {
        SourceLoad::Ready(Ok(self.values.clone()))
    }
}

/// JSON text.
pub(crate) struct JsonSource {
    pub(crate) json: String,
}

impl ConfigurationSource for JsonSource {
    fn name(&self) -> String {
        "JSON".to_owned()
    }

    fn load(&self) -> SourceLoad {
        SourceLoad::Ready(serde_json::from_str(&self.json).map_err(|error| error.to_string()))
    }
}
