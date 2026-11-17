//! The font service for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_runtime::services::fonts::FontService as Fonts;

use crate::di::provider_ts::ServiceClass;
use crate::services::json;

/// The app's fonts, for TSX: `services.getRequiredService(FontService).family("script")`, a
/// `fontFamily` for an alias of the font service (its family and fallbacks), or a family itself.
#[class]
pub struct FontService {
    service: Arc<Fonts>,
}

impl ServiceClass for FontService {
    type Service = Fonts;

    fn wrap(service: Arc<Fonts>) -> Self {
        Self { service }
    }
}

#[class]
impl FontService {
    /// The `fontFamily` for `alias` (its family and fallbacks), or `alias` itself as a family.
    pub fn family(&self, alias: String) -> String {
        self.service.font_family(&alias)
    }

    /// Every alias, resolved: `{ alias, requestedFamily, resolvedFamily, glyphFallbackFamilies }`.
    pub fn aliases(&self) -> JsonValue {
        json(&self.service.resolved_aliases())
    }
}
