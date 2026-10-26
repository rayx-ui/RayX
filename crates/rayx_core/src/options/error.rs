use crate::configuration::ConfigurationError;
use crate::di::ServiceResolutionError;
use std::fmt;

/// Why options could not be made (.NET's `OptionsValidationException` and the binder's and
/// container's exceptions on the way).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionsError {
    /// Validation failed: every failing validation's message, in registration order.
    Validation {
        type_name: &'static str,
        name: String,
        failures: Vec<String>,
    },
    /// A bound configuration section did not bind.
    Bind {
        type_name: &'static str,
        name: String,
        error: ConfigurationError,
    },
    /// A step's dependency is not registered: a service a `*_with` step takes, or the
    /// [`Configuration`](crate::configuration::Configuration) a bind reads.
    Dependency {
        type_name: &'static str,
        name: String,
        error: ServiceResolutionError,
    },
}

impl OptionsError {
    /// The options' type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Validation { type_name, .. }
            | Self::Bind { type_name, .. }
            | Self::Dependency { type_name, .. } => type_name,
        }
    }

    /// The options' name ([`DEFAULT_NAME`](super::DEFAULT_NAME) for the unnamed ones).
    pub fn name(&self) -> &str {
        match self {
            Self::Validation { name, .. }
            | Self::Bind { name, .. }
            | Self::Dependency { name, .. } => name,
        }
    }
}

struct Named<'a>(&'static str, &'a str);

impl fmt::Display for Named<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            "" => write!(formatter, "options `{}`", self.0),
            name => write!(formatter, "options `{}` named `{name}`", self.0),
        }
    }
}

impl fmt::Display for OptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let named = Named(self.type_name(), self.name());
        match self {
            Self::Validation { failures, .. } => {
                write!(
                    formatter,
                    "{named} failed validation: {}",
                    failures.join("; ")
                )
            }
            Self::Bind { error, .. } => write!(formatter, "{named} did not bind: {error}"),
            Self::Dependency { error, .. } => write!(formatter, "{named} need a service: {error}"),
        }
    }
}

impl std::error::Error for OptionsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation { .. } => None,
            Self::Bind { error, .. } => Some(error),
            Self::Dependency { error, .. } => Some(error),
        }
    }
}
