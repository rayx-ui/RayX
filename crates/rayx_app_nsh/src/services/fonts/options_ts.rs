//! The font service's options for TSX (`addFonts`).

use nsh_core::prelude::*;
use rayx_runtime::services::fonts::{FontAlias, FontOptions as Fonts};

use crate::configuration::options_ts::{push, record};

/// The font service's options (`addFonts`), as Rust's `FontOptions` sets them: each method sets
/// one and returns the object, so calls chain.
#[class]
pub struct FontOptions {
    steps: Vec<FontStep>,
}

/// What one `FontOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum FontStep {
    AutoRegisterAssets(bool),
    DefaultAlias(String),
    Alias(FontAlias),
}

impl FontStep {
    pub(crate) fn apply(&self, options: &mut Fonts) {
        match self {
            Self::AutoRegisterAssets(enabled) => {
                options.auto_register_assets(*enabled);
            }
            Self::DefaultAlias(alias) => {
                options.default_alias(alias.clone());
            }
            Self::Alias(alias) => {
                let alias = alias.clone();
                options.alias(alias.alias.clone(), move |configured| *configured = alias);
            }
        }
    }
}

impl FontOptions {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn take(&mut self) -> Vec<FontStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl FontOptions {
    /// Whether every font asset of the asset catalog is registered when the app starts (yes by
    /// default).
    pub fn auto_register_assets(this: Instance<Self>, enabled: bool) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontStep::AutoRegisterAssets(enabled))
        })
    }

    /// The alias the app's text uses by default.
    pub fn default_alias(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontStep::DefaultAlias(alias)))
    }

    /// Adds the alias `alias` (its family is the alias itself unless `configure` names one),
    /// with `configure` setting its family and fallbacks; a `configure` that throws makes this
    /// call throw.
    pub fn alias(
        this: Instance<Self>,
        alias: String,
        #[args(alias)] configure: Option<Callback<Instance<FontAliasOptions>>>,
    ) -> Result<Instance<Self>, String> {
        let mut font = FontAlias::new(alias);
        for step in record(FontAliasOptions::new(), configure, FontAliasOptions::take)? {
            step.apply(&mut font);
        }
        Ok(push(this, |o| o.steps.push(FontStep::Alias(font))))
    }
}

/// A font alias's settings (`FontOptions.alias`'s callback), as Rust's `FontAlias` sets them:
/// each method sets one and returns the object, so calls chain.
#[class]
pub struct FontAliasOptions {
    steps: Vec<FontAliasStep>,
}

/// What one `FontAliasOptions` method set.
#[derive(Clone, Debug)]
pub(crate) enum FontAliasStep {
    Family(String),
    LoadFallbackAlias(String),
    LoadFallbackFamily(String),
    GlyphFallback(String),
}

impl FontAliasStep {
    fn apply(&self, alias: &mut FontAlias) {
        match self {
            Self::Family(family) => alias.family(family.clone()),
            Self::LoadFallbackAlias(other) => alias.load_fallback_alias(other.clone()),
            Self::LoadFallbackFamily(family) => alias.load_fallback_family(family.clone()),
            Self::GlyphFallback(other) => alias.glyph_fallback(other.clone()),
        };
    }
}

impl FontAliasOptions {
    fn new() -> Self {
        Self { steps: Vec::new() }
    }

    fn take(&mut self) -> Vec<FontAliasStep> {
        std::mem::take(&mut self.steps)
    }
}

#[class]
impl FontAliasOptions {
    /// The family the alias names.
    pub fn family(this: Instance<Self>, family: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontAliasStep::Family(family)))
    }

    /// The alias used while this one's font loads, or when it does not.
    pub fn load_fallback_alias(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontAliasStep::LoadFallbackAlias(alias))
        })
    }

    /// The family used while this alias's font loads, or when it does not.
    pub fn load_fallback_family(this: Instance<Self>, family: String) -> Instance<Self> {
        push(this, |o| {
            o.steps.push(FontAliasStep::LoadFallbackFamily(family))
        })
    }

    /// Adds an alias whose font draws the glyphs this one's lacks, in order.
    pub fn glyph_fallback(this: Instance<Self>, alias: String) -> Instance<Self> {
        push(this, |o| o.steps.push(FontAliasStep::GlyphFallback(alias)))
    }
}
