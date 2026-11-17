use std::fmt;

/// Why configuration did not load, or a value did not convert.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationError {
    /// A source failed to load.
    Source {
        source_name: String,
        message: String,
    },
    /// A source's tree is not an object (`null` is allowed: it contributes nothing).
    NotAnObject {
        source_name: String,
        kind: &'static str,
    },
    /// A source loads asynchronously: build with [`ConfigurationBuilder::build_async`], reload
    /// with [`Configuration::reload_async`].
    ///
    /// [`ConfigurationBuilder::build_async`]: super::ConfigurationBuilder::build_async
    /// [`Configuration::reload_async`]: super::Configuration::reload_async
    AsyncSource { source_name: String },
    /// The value at `path` did not convert to `type_name`.
    Get {
        path: String,
        type_name: &'static str,
        message: String,
    },
    /// The section at `path` did not bind to `type_name` (its value with the section over it did
    /// not convert back).
    Bind {
        path: String,
        type_name: &'static str,
        message: String,
    },
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source {
                source_name,
                message,
            } => write!(
                formatter,
                "configuration source `{source_name}` did not load: {message}"
            ),
            Self::NotAnObject { source_name, kind } => write!(
                formatter,
                "configuration source `{source_name}` must load an object, not {kind}"
            ),
            Self::AsyncSource { source_name } => write!(
                formatter,
                "configuration source `{source_name}` loads asynchronously: build or reload the configuration with the async method"
            ),
            Self::Get {
                path,
                type_name,
                message,
            } => write!(
                formatter,
                "configuration value `{path}` is not a `{type_name}`: {message}"
            ),
            Self::Bind {
                path,
                type_name,
                message,
            } => write!(
                formatter,
                "configuration section `{path}` does not bind to `{type_name}`: {message}"
            ),
        }
    }
}

impl std::error::Error for ConfigurationError {}
