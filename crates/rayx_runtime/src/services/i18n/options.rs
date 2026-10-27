use serde::{Deserialize, Serialize};

/// The configuration section [`I18nOptions`] bind from.
pub const I18N_CONFIG_PATH: &str = "RayX:Services:I18N";

/// What a message no bundle has shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingMessagePolicy {
    /// The message's id, so the gap is visible and the screen still reads.
    #[default]
    Id,
    /// Nothing.
    Empty,
}

/// Which languages the app has, which one it falls back to, and where its messages are.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct I18nOptions {
    /// The language messages fall back to last, and the one shown before the user's is known
    /// (`en`).
    pub default_language: String,
    /// Languages tried, in order, after the user's and before the default one.
    pub fallback_languages: Vec<String>,
    /// The directory of the asset catalog's translations (`locales`): `locales/<language>.json`
    /// or `locales/<language>/<namespace>.json`.
    pub resources_path: String,
    /// The namespace whose messages keep their ids as they are (`translation`); another
    /// namespace's ids are `<namespace>.<id>`.
    pub default_namespace: String,
    /// Loads every translation of the asset catalog when the app starts.
    pub auto_load_assets: bool,
    /// Follows the user's preferred languages and clock as the platform reports them, until the
    /// app picks a language itself.
    pub follow_platform: bool,
    /// What a missing message shows.
    pub missing_message: MissingMessagePolicy,
    /// Wraps placeholders whose direction is not the message's in Unicode isolates
    /// (MessageFormat 2's default bidi strategy).
    pub bidi_isolation: bool,
}

impl Default for I18nOptions {
    fn default() -> Self {
        Self {
            default_language: "en".to_owned(),
            fallback_languages: Vec::new(),
            resources_path: "locales".to_owned(),
            default_namespace: "translation".to_owned(),
            auto_load_assets: true,
            follow_platform: true,
            missing_message: MissingMessagePolicy::Id,
            bidi_isolation: false,
        }
    }
}

impl I18nOptions {
    pub fn default_language(&mut self, language: impl Into<String>) -> &mut Self {
        self.default_language = language.into();
        self
    }

    pub fn fallback_language(&mut self, language: impl Into<String>) -> &mut Self {
        self.fallback_languages.push(language.into());
        self
    }

    pub fn resources_path(&mut self, path: impl Into<String>) -> &mut Self {
        self.resources_path = path.into();
        self
    }

    pub fn auto_load_assets(&mut self, enabled: bool) -> &mut Self {
        self.auto_load_assets = enabled;
        self
    }

    pub fn follow_platform(&mut self, enabled: bool) -> &mut Self {
        self.follow_platform = enabled;
        self
    }

    pub fn missing_message(&mut self, policy: MissingMessagePolicy) -> &mut Self {
        self.missing_message = policy;
        self
    }

    pub fn bidi_isolation(&mut self, enabled: bool) -> &mut Self {
        self.bidi_isolation = enabled;
        self
    }

    /// Whether every language tag reads.
    pub fn is_valid(&self) -> bool {
        std::iter::once(&self.default_language)
            .chain(&self.fallback_languages)
            .all(|tag| dxn_intl::parse_locale(tag).is_ok())
    }
}
