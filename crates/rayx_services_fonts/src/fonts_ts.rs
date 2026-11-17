//! The font service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::services::fonts::FontService as Fonts;
use rayx_app_nsh::ts::{ServiceClass, json};

/// The app's fonts, for TSX: `services.getRequiredService(FontService).family("script")`, a
/// `fontFamily` for an alias of the font service (its family and fallbacks), or a family itself.
#[class]
pub struct FontService {
    service: Arc<dyn Fonts>,
}

impl ServiceClass for FontService {
    type Service = dyn Fonts;

    fn wrap(service: Arc<dyn Fonts>) -> Self {
        Self { service }
    }
}

#[class]
impl FontService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): FontServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// The `fontFamily` for `alias` (its family and fallbacks), or `alias` itself as a family.
    pub fn family(&self, alias: String) -> String {
        self.service.font_family(&alias)
    }

    /// Every alias, resolved: `{ alias, requested_family, resolved_family,
    /// glyph_fallback_families }`.
    pub fn aliases(&self) -> JsonValue {
        json(&self.service.resolved_aliases())
    }
}
