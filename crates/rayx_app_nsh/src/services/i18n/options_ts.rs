//! The i18n service's options for TSX (`addI18n`).

use nsh_core::prelude::*;
use rayx_runtime::services::i18n::{I18nOptions as I18n, MissingMessagePolicy};

use crate::configuration::options_ts::push;

/// The i18n service's options (`addI18n`), as Rust's `I18nOptions` sets them: each method sets
/// one and returns the object, so calls chain.
#[class]
pub struct I18nOptions {
    steps: Vec<I18nStep>,
}

/// What one `I18nOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum I18nStep {
    DefaultLanguage(String),
    FallbackLanguage(String),
    ResourcesPath(String),
    AutoLoadAssets(bool),
    FollowPlatform(bool),
    MissingMessage(MissingMessagePolicy),
    BidiIsolation(bool),
}

impl I18nStep {
    pub(crate) fn apply(&self, options: &mut I18n) {
        match self {
            Self::DefaultLanguage(language) => options.default_language(language.clone()),
            Self::FallbackLanguage(language) => options.fallback_language(language.clone()),
            Self::ResourcesPath(path) => options.resources_path(path.clone()),
            Self::AutoLoadAssets(enabled) => options.auto_load_assets(*enabled),
            Self::FollowPlatform(enabled) => options.follow_platform(*enabled),
            Self::MissingMessage(policy) => options.missing_message(*policy),
            Self::BidiIsolation(enabled) => options.bidi_isolation(*enabled),
        };
    }
}

impl I18nOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<I18nStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl I18nOptions {
    /// The language messages fall back to last, and the one shown before the user's is known
    /// (`"en"` by default).
    pub fn default_language(this: Instance<Self>, language: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::DefaultLanguage(language)))
    }

    /// Adds a language tried, in order, after the user's and before the default one.
    pub fn fallback_language(this: Instance<Self>, language: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::FallbackLanguage(language)))
    }

    /// The directory of the asset catalog's translations (`"locales"` by default).
    pub fn resources_path(this: Instance<Self>, path: String) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::ResourcesPath(path)))
    }

    /// Whether every translation of the asset catalog loads when the app starts (yes by
    /// default).
    pub fn auto_load_assets(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::AutoLoadAssets(enabled)))
    }

    /// Whether the app follows the user's preferred languages and clock as the platform reports
    /// them, until it picks a language itself (yes by default).
    pub fn follow_platform(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::FollowPlatform(enabled)))
    }

    /// What a missing message shows: `"id"` (its id, the default) or `"empty"` (nothing).
    pub fn missing_message(this: Instance<Self>, policy: String) -> Result<Instance<Self>, String> {
        let policy = match policy.as_str() {
            "id" => MissingMessagePolicy::Id,
            "empty" => MissingMessagePolicy::Empty,
            other => {
                return Err(format!(
                    "missingMessage takes \"id\" or \"empty\", not {other:?}"
                ));
            }
        };
        Ok(push(this, |o| {
            o.steps.push(I18nStep::MissingMessage(policy))
        }))
    }

    /// Whether placeholders whose direction is not the message's are wrapped in Unicode
    /// isolates (no by default).
    pub fn bidi_isolation(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| o.steps.push(I18nStep::BidiIsolation(enabled)))
    }
}
