use serde::{Deserialize, Serialize};

/// How bad a font problem is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum FontDiagnosticSeverity {
    Warning,
    Error,
}

/// What went wrong.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum FontDiagnosticCode {
    /// A font asset the manifest lists did not load.
    MissingAsset,
    /// The platform refused a font (no font data, or no family name in it).
    RegistrationFailed,
    /// An alias's family did not load, so its load fallback stands in.
    LoadFallbackSubstitution,
}

/// A problem found registering fonts or resolving aliases.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontDiagnostic {
    pub severity: FontDiagnosticSeverity,
    pub code: FontDiagnosticCode,
    pub message: String,
    pub logical_path: Option<String>,
    pub alias: Option<String>,
    pub fallback: Option<String>,
}

impl FontDiagnostic {
    pub fn error(code: FontDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: FontDiagnosticSeverity::Error,
            code,
            message: message.into(),
            logical_path: None,
            alias: None,
            fallback: None,
        }
    }

    pub fn warning(code: FontDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: FontDiagnosticSeverity::Warning,
            ..Self::error(code, message)
        }
    }

    pub fn with_logical_path(mut self, logical_path: impl Into<String>) -> Self {
        self.logical_path = Some(logical_path.into());
        self
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    pub fn with_fallback(mut self, fallback: impl Into<String>) -> Self {
        self.fallback = Some(fallback.into());
        self
    }
}
