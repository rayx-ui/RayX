//! Fonts: registered with the platform while the app runs (the asset catalog's font files at
//! startup, others from their data), and aliases the app names fonts by, resolved into font
//! family lists with load and glyph fallbacks.

mod app_ext;
mod diagnostics;
mod options;
mod registrar;
mod service;

pub use app_ext::FontAppBuilderExt;
pub use diagnostics::{FontDiagnostic, FontDiagnosticCode, FontDiagnosticSeverity};
pub use options::{
    FONT_CONFIG_PATH, FontAlias, FontLoadFallback, FontLoadFallbackKind, FontOptions,
    FontOptionsError,
};
pub use registrar::FontRegistrar;
pub use service::{FontService, FontServiceError, RegisteredFontFace, ResolvedFontAlias};
