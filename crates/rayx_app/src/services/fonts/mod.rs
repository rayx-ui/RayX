//! Fonts: the fonts registered with the platform while the app runs (the asset catalog's font
//! files at startup, others from their data) and the aliases the app names fonts by, resolved
//! into font family lists with load and glyph fallbacks, published by a [`FontService`]
//! implementation as [`FontState`].
//!
//! RayX's implementation is `rayx_services_fonts`' (`add_fonts(..)`), which registers with NSH
//! and reads the font files through the app's `dyn AssetService`; an app may register its own,
//! in Rust or in TS. Resolving an alias ([`FontState::resolve_alias`],
//! [`FontState::font_family`], [`FontState::default_font`]) is synchronous for any
//! implementation, and so are the same helpers on `dyn FontService`.

mod diagnostics;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use diagnostics::{FontDiagnostic, FontDiagnosticCode, FontDiagnosticSeverity};

use super::ContractFailure;
use crate::published::StateCell;

/// A font the service registered.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegisteredFontFace {
    /// The asset it came from (`@<package>/<path>`), or `None` for one registered from bytes.
    pub logical_path: Option<String>,
    /// Its family name, as the platform read it.
    pub family: String,
}

/// An alias resolved against the fonts that loaded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFontAlias {
    pub alias: String,
    pub requested_family: String,
    /// The family, or its load fallback's when it did not load.
    pub resolved_family: String,
    /// The glyph fallbacks' families, in order.
    pub glyph_fallback_families: Vec<String>,
}

impl ResolvedFontAlias {
    /// The family and its glyph fallbacks as one font family list (`Inter, Noto Color Emoji`),
    /// for NSH's `font_family` / `fontFamily`, which take fallback lists.
    pub fn family_list(&self) -> String {
        std::iter::once(&self.resolved_family)
            .chain(&self.glyph_fallback_families)
            .map(|family| {
                if family.contains([',', '"']) || family.trim() != family {
                    format!("\"{}\"", family.replace('"', "\\\""))
                } else {
                    family.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The fonts now: those registered, every alias resolved against them, the default alias and
/// what registering and resolving found wrong.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontState {
    /// The fonts registered so far.
    pub faces: Vec<RegisteredFontFace>,
    /// Every alias, resolved against the fonts registered so far.
    pub aliases: Vec<ResolvedFontAlias>,
    /// The alias the app's text uses unless it names another.
    pub default_alias: Option<String>,
    /// What registering fonts and resolving aliases found wrong.
    pub diagnostics: Vec<FontDiagnostic>,
}

impl FontState {
    /// The alias `alias`, resolved.
    pub fn resolve_alias(&self, alias: &str) -> Option<&ResolvedFontAlias> {
        self.aliases.iter().find(|resolved| resolved.alias == alias)
    }

    /// The default alias, resolved.
    pub fn default_font(&self) -> Option<&ResolvedFontAlias> {
        self.default_alias
            .as_deref()
            .and_then(|alias| self.resolve_alias(alias))
    }

    /// The font family list for `alias_or_family`: an alias's resolved list, or the name itself.
    pub fn font_family(&self, alias_or_family: &str) -> String {
        self.resolve_alias(alias_or_family).map_or_else(
            || alias_or_family.to_owned(),
            ResolvedFontAlias::family_list,
        )
    }
}

/// Why a font command failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FontServiceError {
    /// The platform cannot register fonts.
    NoRegistrar,
    /// Registering the catalog's fonts needs an asset service, and none is registered.
    MissingAssetService,
    /// The platform refused the font (no font data, or no family name in it).
    Registration(String),
    /// The call failed (a TS implementation threw, the engine is gone).
    Failed(ContractFailure),
}

impl fmt::Display for FontServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRegistrar => f.write_str("the platform cannot register fonts"),
            Self::MissingAssetService => {
                f.write_str("the font service needs the asset service to register font assets")
            }
            Self::Registration(reason) => write!(f, "the font could not be registered: {reason}"),
            Self::Failed(failure) => failure.fmt(f),
        }
    }
}

impl std::error::Error for FontServiceError {}

impl From<ContractFailure> for FontServiceError {
    fn from(failure: ContractFailure) -> Self {
        Self::Failed(failure)
    }
}

/// The font service: the fonts registered and the aliases resolved against them, published as
/// [`FontState`], and registering more.
#[rayx_app_macros::contract(ts = "FontServiceContract")]
pub trait FontService {
    /// The fonts and aliases now.
    #[state(ts = "FontState")]
    fn fonts(&self) -> &StateCell<FontState>;

    /// Registers a font from its data while the app runs (one downloaded, picked or made: TrueType,
    /// OpenType, WOFF or WOFF2), resolving the aliases again; resolves to its family name.
    #[ts("(data: number[]): string")]
    async fn register_font(&self, data: Vec<u8>) -> Result<String, FontServiceError>;

    /// Registers the asset catalog's font files, then resolves the aliases (RayX's
    /// implementation does at startup unless its options turn it off). A font that does not
    /// load or register is a diagnostic.
    async fn register_font_assets(&self) -> Result<(), FontServiceError>;
}

impl dyn FontService {
    /// The fonts registered so far.
    pub fn registered_fonts(&self) -> Vec<RegisteredFontFace> {
        self.fonts().get().faces.clone()
    }

    /// Every alias, resolved.
    pub fn resolved_aliases(&self) -> Vec<ResolvedFontAlias> {
        self.fonts().get().aliases.clone()
    }

    /// The alias `alias`, resolved.
    pub fn resolve_alias(&self, alias: &str) -> Option<ResolvedFontAlias> {
        self.fonts().get().resolve_alias(alias).cloned()
    }

    /// The default alias, resolved.
    pub fn default_font(&self) -> Option<ResolvedFontAlias> {
        self.fonts().get().default_font().cloned()
    }

    /// The font family list for `alias_or_family`: an alias's resolved list, or the name itself.
    pub fn font_family(&self, alias_or_family: &str) -> String {
        self.fonts().get().font_family(alias_or_family)
    }

    /// What registering fonts and resolving aliases found wrong.
    pub fn diagnostics(&self) -> Vec<FontDiagnostic> {
        self.fonts().get().diagnostics.clone()
    }
}
