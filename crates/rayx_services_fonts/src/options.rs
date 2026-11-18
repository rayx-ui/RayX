use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The configuration section [`FontOptions`] bind from.
pub const FONT_CONFIG_PATH: &str = "RayX:Services:Fonts";

/// Which fonts the font service registers, and the aliases the app names fonts by.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontOptions {
    /// Registers every font asset of the asset catalog when the app starts.
    pub auto_register_assets: bool,
    /// The alias the app's text uses by default.
    pub default_alias: Option<String>,
    pub aliases: Vec<FontAlias>,
}

impl Default for FontOptions {
    fn default() -> Self {
        Self {
            auto_register_assets: true,
            default_alias: None,
            aliases: Vec::new(),
        }
    }
}

impl FontOptions {
    pub fn auto_register_assets(&mut self, enabled: bool) -> &mut Self {
        self.auto_register_assets = enabled;
        self
    }

    pub fn default_alias(&mut self, alias: impl Into<String>) -> &mut Self {
        self.default_alias = Some(alias.into());
        self
    }

    pub fn alias(
        &mut self,
        alias: impl Into<String>,
        configure: impl FnOnce(&mut FontAlias),
    ) -> &mut Self {
        let mut alias = FontAlias::new(alias);
        configure(&mut alias);
        self.aliases.push(alias);
        self
    }

    pub fn normalized(mut self) -> Result<Self, FontOptionsError> {
        for alias in &mut self.aliases {
            alias.alias = normalized_required("font alias", &alias.alias)?;
            if alias.family.trim().is_empty() {
                alias.family = alias.alias.clone();
            }
            alias.family = normalized_required("font family", &alias.family)?;
            if let Some(fallback) = &mut alias.load_fallback {
                fallback.target = normalized_required("font load fallback", &fallback.target)?;
            }
            for fallback in &mut alias.glyph_fallbacks {
                *fallback = normalized_required("font glyph fallback", fallback)?;
            }
        }
        if let Some(default_alias) = &mut self.default_alias {
            *default_alias = normalized_required("default font alias", default_alias)?;
        }
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), FontOptionsError> {
        let mut aliases = HashSet::new();
        for alias in &self.aliases {
            if !aliases.insert(alias.alias.clone()) {
                return Err(FontOptionsError::DuplicateAlias(alias.alias.clone()));
            }
        }

        if let Some(default_alias) = &self.default_alias
            && !aliases.contains(default_alias)
        {
            return Err(FontOptionsError::UnknownDefaultAlias(default_alias.clone()));
        }

        for alias in &self.aliases {
            for fallback in &alias.glyph_fallbacks {
                if !aliases.contains(fallback) {
                    return Err(FontOptionsError::UnknownGlyphFallbackAlias {
                        alias: alias.alias.clone(),
                        fallback: fallback.clone(),
                    });
                }
            }
            if let Some(FontLoadFallback {
                kind: FontLoadFallbackKind::Alias,
                target,
            }) = &alias.load_fallback
                && !aliases.contains(target)
            {
                return Err(FontOptionsError::UnknownLoadFallbackAlias {
                    alias: alias.alias.clone(),
                    fallback: target.clone(),
                });
            }
        }

        Ok(())
    }
}

/// A name the app gives a font: the family it stands for, what stands in when that family did
/// not load, and the aliases whose families draw the glyphs it lacks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontAlias {
    pub alias: String,
    /// The family; the alias itself when empty.
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub load_fallback: Option<FontLoadFallback>,
    /// Aliases, in order.
    #[serde(default)]
    pub glyph_fallbacks: Vec<String>,
}

impl FontAlias {
    pub fn new(alias: impl Into<String>) -> Self {
        let alias = alias.into();
        Self {
            alias: alias.clone(),
            family: alias,
            load_fallback: None,
            glyph_fallbacks: Vec::new(),
        }
    }

    pub fn family(&mut self, family: impl Into<String>) -> &mut Self {
        self.family = family.into();
        self
    }

    pub fn load_fallback_alias(&mut self, alias: impl Into<String>) -> &mut Self {
        self.load_fallback = Some(FontLoadFallback::alias(alias));
        self
    }

    pub fn load_fallback_family(&mut self, family: impl Into<String>) -> &mut Self {
        self.load_fallback = Some(FontLoadFallback::family(family));
        self
    }

    pub fn glyph_fallback(&mut self, alias: impl Into<String>) -> &mut Self {
        self.glyph_fallbacks.push(alias.into());
        self
    }
}

/// What stands in for an alias whose family did not load.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FontLoadFallback {
    pub kind: FontLoadFallbackKind,
    pub target: String,
}

impl FontLoadFallback {
    pub fn alias(alias: impl Into<String>) -> Self {
        Self {
            kind: FontLoadFallbackKind::Alias,
            target: alias.into(),
        }
    }

    pub fn family(family: impl Into<String>) -> Self {
        Self {
            kind: FontLoadFallbackKind::Family,
            target: family.into(),
        }
    }
}

/// Whether a load fallback names another alias or a family.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FontLoadFallbackKind {
    Alias,
    Family,
}

/// Why font options are invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FontOptionsError {
    #[error("{field} must not be empty")]
    EmptyValue { field: &'static str },
    #[error("duplicate font alias `{0}`")]
    DuplicateAlias(String),
    #[error("default font alias `{0}` is not configured")]
    UnknownDefaultAlias(String),
    #[error("font alias `{alias}` uses unknown load fallback alias `{fallback}`")]
    UnknownLoadFallbackAlias { alias: String, fallback: String },
    #[error("font alias `{alias}` uses unknown glyph fallback alias `{fallback}`")]
    UnknownGlyphFallbackAlias { alias: String, fallback: String },
}

fn normalized_required(field: &'static str, value: &str) -> Result<String, FontOptionsError> {
    let value = value.trim();
    if value.is_empty() {
        Err(FontOptionsError::EmptyValue { field })
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_options_defaults_auto_register_assets() {
        let options = FontOptions::default();
        assert!(options.auto_register_assets);
        assert!(options.default_alias.is_none());
        assert!(options.aliases.is_empty());
    }

    #[test]
    fn font_options_reject_duplicate_aliases() {
        let mut options = FontOptions::default();
        options.alias("inter", |_| {});
        options.alias(" inter ", |_| {});

        assert_eq!(
            options.normalized().unwrap_err(),
            FontOptionsError::DuplicateAlias("inter".to_string())
        );
    }

    #[test]
    fn font_options_reject_unknown_default_alias() {
        let mut options = FontOptions::default();
        options.default_alias("missing");

        assert_eq!(
            options.normalized().unwrap_err(),
            FontOptionsError::UnknownDefaultAlias("missing".to_string())
        );
    }

    #[test]
    fn font_options_reject_unknown_glyph_fallback_alias() {
        let mut options = FontOptions::default();
        options.alias("inter", |alias| {
            alias.family("Inter").glyph_fallback("emoji");
        });
        options.default_alias("inter");

        assert_eq!(
            options.normalized().unwrap_err(),
            FontOptionsError::UnknownGlyphFallbackAlias {
                alias: "inter".to_string(),
                fallback: "emoji".to_string()
            }
        );
    }

    #[test]
    fn font_options_accept_alias_fallbacks_and_default() -> Result<(), FontOptionsError> {
        let mut options = FontOptions::default();
        options.alias("emoji", |alias| {
            alias
                .family("Segoe UI Emoji")
                .load_fallback_family("Noto Color Emoji");
        });
        options.alias("inter", |alias| {
            alias
                .family("Inter")
                .load_fallback_alias("emoji")
                .glyph_fallback("emoji");
        });
        options.default_alias("inter");

        let options = options.normalized()?;
        assert_eq!(options.default_alias.as_deref(), Some("inter"));
        assert_eq!(options.aliases.len(), 2);
        Ok(())
    }
}
