//! Assets: the app's files (images, fonts, messages, data) listed by its packages' manifests in
//! an [`AssetCatalog`], published by an [`AssetService`] implementation, and read from it by
//! logical path (`@<package>/<path>`), as bytes or text.
//!
//! RayX's implementation is `rayx_services_assets`' (`add_assets(..)`), which reads the files
//! with NSH; an app may register its own `dyn AssetService`, in Rust or in TS. Looking assets up
//! in the catalog ([`AssetCatalog::get`], [`AssetCatalog::query`], [`AssetDescriptor::uri`]) is
//! synchronous for any implementation, and so are the helpers on `dyn AssetService`.

mod catalog;
mod error;
pub mod glob;
mod kind;
mod manifest;
mod path;

pub use catalog::{
    AssetCatalog, AssetCatalogBuilder, AssetDescriptor, AssetDiagnostic, AssetDiagnosticSeverity,
    AssetQuery,
};
pub use error::{AssetError, Result};
pub use glob::{GlobSpecificity, capture_subdir, glob_matches, is_glob};
pub use kind::AssetKind;
pub use manifest::{AssetManifest, AssetManifestEntry, WhenValue};
pub use path::{DENSITY_SCALES, normalize, normalize_namespace};

use serde::de::DeserializeOwned;

use crate::published::StateCell;

/// The asset service: the app's catalog, published once loaded (`None` until then), and the
/// files read from it.
#[rayx_app_macros::contract(ts = "AssetServiceContract")]
pub trait AssetService {
    /// The catalog, once loaded (`None` until then), with what reading the manifests found
    /// wrong.
    #[state(ts = "AssetCatalog | null")]
    fn catalog(&self) -> &StateCell<Option<AssetCatalog>>;

    /// Loads the catalog from the packages' manifests (the app's start does), publishing it.
    async fn load_catalog(&self) -> Result<(), AssetError>;

    /// The bytes of the asset at `path`.
    #[ts("(path: string): number[] | Uint8Array")]
    async fn load_bytes(&self, path: String) -> Result<Vec<u8>, AssetError>;

    /// The UTF-8 text of the asset at `path`.
    async fn load_text(&self, path: String) -> Result<String, AssetError>;

    /// Forgets the bytes read so far (the next reads read the files again).
    fn clear_cache(&self);
}

impl dyn AssetService {
    /// The catalog now, or [`AssetError::NotLoaded`].
    pub fn loaded_catalog(&self) -> Result<std::sync::Arc<Option<AssetCatalog>>> {
        let catalog = self.catalog().get();
        if catalog.is_some() {
            Ok(catalog)
        } else {
            Err(AssetError::NotLoaded)
        }
    }

    /// The asset at `path` (a logical path, or a path in the first package that has it).
    pub fn descriptor(&self, path: &str) -> Result<AssetDescriptor> {
        let catalog = self.loaded_catalog()?;
        let catalog = catalog.as_ref().as_ref().ok_or(AssetError::NotLoaded)?;
        catalog
            .get(path)
            .map(|descriptor| (**descriptor).clone())
            .ok_or_else(|| AssetError::NotFound(path.to_owned()))
    }

    /// The assets `query` matches.
    pub fn query(&self, query: &AssetQuery) -> Result<Vec<AssetDescriptor>> {
        let catalog = self.loaded_catalog()?;
        let catalog = catalog.as_ref().as_ref().ok_or(AssetError::NotLoaded)?;
        Ok(catalog
            .query(query)
            .into_iter()
            .map(|descriptor| (*descriptor).clone())
            .collect())
    }

    /// What reading the manifests found wrong (nothing before the catalog loads).
    pub fn diagnostics(&self) -> Vec<AssetDiagnostic> {
        self.catalog()
            .get()
            .as_ref()
            .as_ref()
            .map(|catalog| catalog.diagnostics().to_vec())
            .unwrap_or_default()
    }

    /// The `asset:/` URL of the asset at `path` (an `<Image>`'s source).
    pub fn uri(&self, path: &str) -> Result<String> {
        self.descriptor(path).map(|descriptor| descriptor.uri())
    }

    /// The URL of the asset's variant for `scale` (its `@2x` file when it has one).
    pub fn uri_for_scale(&self, path: &str, scale: f32) -> Result<String> {
        self.descriptor(path)
            .map(|descriptor| descriptor.uri_for_scale(scale))
    }

    /// The asset at `path` as JSON, decoded into `T`.
    pub async fn load_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let text = self.load_text(path.to_owned()).await?;
        serde_json::from_str(&text).map_err(|error| AssetError::Decode {
            path: path.to_owned(),
            expected: "JSON",
            reason: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;

    use super::*;

    #[test]
    fn assets_kind_infers_semantic_kinds_from_paths() {
        assert_eq!(
            AssetKind::infer_from_path("icons/nav/home.svg"),
            AssetKind::Icon
        );
        assert_eq!(AssetKind::infer_from_path("icons/app.png"), AssetKind::Icon);
        assert_eq!(
            AssetKind::infer_from_path("icons/app.webp"),
            AssetKind::Icon
        );
        assert_eq!(
            AssetKind::infer_from_path("illustrations/empty.svg"),
            AssetKind::Illustration
        );
        assert_eq!(
            AssetKind::infer_from_path("illustrations/hero.webp"),
            AssetKind::Illustration
        );
        assert_eq!(
            AssetKind::infer_from_path("locales/en.json"),
            AssetKind::Locale
        );
        assert_eq!(
            AssetKind::infer_from_path("i18n/hr/shell.json"),
            AssetKind::Locale
        );
        assert_eq!(
            AssetKind::infer_from_path("keymaps/default.json"),
            AssetKind::Keymap
        );
        assert_eq!(
            AssetKind::infer_from_path("keybindings/studio.json"),
            AssetKind::Keymap
        );
        assert_eq!(
            AssetKind::infer_from_path("themes/dark.json"),
            AssetKind::Theme
        );
        assert_eq!(
            AssetKind::infer_from_path("tokens/brand.dtcg.json"),
            AssetKind::Theme
        );
        assert_eq!(
            AssetKind::infer_from_path("fonts/Inter.ttf"),
            AssetKind::Font
        );
        assert_eq!(
            AssetKind::infer_from_path("fonts/Inter.woff2"),
            AssetKind::Font
        );
        // Paths outside semantic folders keep legacy kinds.
        assert_eq!(
            AssetKind::infer_from_path("images/logo.png"),
            AssetKind::Image
        );
        assert_eq!(AssetKind::infer_from_path("art/mark.svg"), AssetKind::Svg);
    }

    #[test]
    fn assets_kind_validates_extensions_and_density_variants() {
        assert!(AssetKind::Font.validate_extension("fonts/a.ttf").is_ok());
        assert!(AssetKind::Font.validate_extension("fonts/a.otf").is_ok());
        assert!(AssetKind::Font.validate_extension("fonts/a.woff").is_ok());
        assert!(AssetKind::Font.validate_extension("fonts/a.woff2").is_ok());
        assert!(AssetKind::Font.validate_extension("fonts/a.png").is_err());

        for kind in [AssetKind::Icon, AssetKind::Illustration] {
            assert!(kind.validate_extension("a.svg").is_ok());
            assert!(kind.validate_extension("a.png").is_ok());
            assert!(kind.validate_extension("a.webp").is_ok());
            assert!(kind.validate_extension("a.jpg").is_err());
            assert!(kind.has_density_variants());
            assert!(kind.has_density_variants_for_path("a.png"));
            assert!(kind.has_density_variants_for_path("a.webp"));
            assert!(!kind.has_density_variants_for_path("a.svg"));
        }

        for kind in [AssetKind::Locale, AssetKind::Keymap, AssetKind::Theme] {
            assert!(kind.validate_extension("a.json").is_ok());
            assert!(kind.validate_extension("a.toml").is_err());
            assert!(!kind.has_density_variants());
            assert!(!kind.has_density_variants_for_path("a.json"));
        }

        assert!(AssetKind::Image.has_density_variants());
        assert!(AssetKind::Image.has_density_variants_for_path("hero.png"));
        assert!(!AssetKind::Svg.has_density_variants_for_path("mark.svg"));
    }

    #[test]
    fn assets_glob_matching_braces_specificity_and_subdir_capture() {
        assert!(is_glob("icons/*.svg"));
        assert!(is_glob("icons/?.svg"));
        assert!(is_glob("icons/{a,b}.svg"));
        assert!(!is_glob("icons/home.svg"));

        assert!(glob_matches("icons/**/*.svg", "icons/home.svg"));
        assert!(glob_matches("icons/**/*.svg", "icons/nav/home.svg"));
        assert!(glob_matches("icons/**/*.svg", "icons/nav/arrows/left.svg"));
        assert!(!glob_matches("icons/*.svg", "icons/nav/home.svg"));
        assert!(glob_matches("icons/?.svg", "icons/x.svg"));
        assert!(!glob_matches("icons/?.svg", "icons/xy.svg"));
        assert!(glob_matches(
            "fonts/{sans,mono}/*.{ttf,woff2}",
            "fonts/sans/Inter.woff2"
        ));
        assert!(!glob_matches(
            "fonts/{sans,mono}/*.{ttf,woff2}",
            "fonts/serif/Georgia.ttf"
        ));

        assert_eq!(
            capture_subdir("icons/**/*.svg", "icons/nav/home.svg"),
            "nav"
        );
        assert_eq!(
            capture_subdir("icons/**/*.svg", "icons/nav/arrows/left.svg"),
            "nav/arrows"
        );
        assert_eq!(capture_subdir("icons/**/*.svg", "icons/close.svg"), "");
        assert_eq!(
            capture_subdir("fonts/{sans,mono}/*.ttf", "fonts/mono/JB.ttf"),
            "mono"
        );

        assert!(GlobSpecificity::of("**/*.svg") < GlobSpecificity::of("icons/**/*.svg"));
        assert!(GlobSpecificity::of("icons/**/*.svg") < GlobSpecificity::of("icons/nav/**/*.svg"));
        assert!(GlobSpecificity::of("icons/nav/**/*.svg") < GlobSpecificity::of("icons/*.svg"));
        assert!(GlobSpecificity::of("icons/*.svg") < GlobSpecificity::of("icons/nav/*.svg"));
        assert!(GlobSpecificity::of("icons/nav/*.svg") < GlobSpecificity::of("icons/nav/home.svg"));
    }

    #[test]
    fn assets_normalize_namespace_validates_segments() {
        assert_eq!(
            normalize_namespace("icons/navigation").unwrap(),
            "icons/navigation"
        );
        assert_eq!(normalize_namespace("fonts").unwrap(), "fonts");
        assert!(normalize_namespace("").is_err());
        assert!(normalize_namespace("/icons").is_err());
        assert!(normalize_namespace("icons/").is_err());
        assert!(normalize_namespace("icons//nav").is_err());
        assert!(normalize_namespace("icons/../nav").is_err());
        assert!(normalize_namespace("icons/./nav").is_err());
        assert!(normalize_namespace("icons\\nav").is_err());
    }

    #[test]
    fn assets_catalog_glob_expansion_specificity_merge_and_namespaces() {
        let manifest = AssetManifest::parse(
            r#"
namespaces = ["illustrations/empty", "icons/custom"]
files = ["fonts/**/*-OFL.txt"]

# Exact entry declared BEFORE globs to prove specificity ordering wins over file order.
[[asset]]
path = "icons/nav/home.svg"
title = "Home"
tags = ["primary"]
custom = { weight = 2, exact = true }

# More specific glob.
[[asset]]
path = "icons/nav/*.svg"
tags = ["nav", "ui"]
description = "Navigation icon"
custom = { weight = 1, scope = "nav" }

# Broad glob with exclude and {subdir} capture.
[[asset]]
path = "icons/**/*.svg"
exclude = ["icons/draft/**"]
namespace = "icons/{subdir}"
kind = "icon"
tags = ["ui"]
custom = { scope = "all", base = true }

# Context predicate (`when`) and logical `alias`.
[[asset]]
path = "brand/acme/logo.svg"
kind = "illustration"
namespace = "brand"
when = { platform = ["ios", "macos"], brand = "acme" }
alias = "brand/logo.svg"

# Raster icon with density variants.
[[asset]]
path = "icons/raster/*.png"
kind = "icon"
namespace = "icons/raster"
"#,
        )
        .unwrap();

        let available: Vec<String> = [
            "assets.toml",
            ".DS_Store",
            "icons/.hidden.svg",
            "fonts/sans/Inter-OFL.txt",
            "icons/close.svg",
            "icons/nav/home.svg",
            "icons/nav/back.svg",
            "icons/draft/wip.svg",
            "icons/raster/star.png",
            "icons/raster/star@2x.png",
            "brand/acme/logo.svg",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        let mut builder = AssetCatalog::builder();
        builder.add_manifest_with_files("app", &manifest, &available);
        let catalog = builder.build();

        assert!(
            catalog.diagnostics().is_empty(),
            "{:?}",
            catalog.diagnostics()
        );
        // `assets.toml`, hidden files, and excluded `icons/draft/wip.svg` are not in catalog.
        assert!(catalog.get("@app/assets.toml").is_none());
        assert!(catalog.get("@app/.DS_Store").is_none());
        assert!(catalog.get("@app/icons/.hidden.svg").is_none());
        assert!(catalog.get("@app/icons/draft/wip.svg").is_none());

        // Shorthand `files` glob expanded.
        assert_eq!(
            catalog.get("@app/fonts/sans/Inter-OFL.txt").unwrap().kind,
            AssetKind::Text
        );

        // `{subdir}` collapses cleanly when file is directly under `icons/`.
        let close = catalog.get("@app/icons/close.svg").unwrap();
        assert_eq!(close.kind, AssetKind::Icon);
        assert_eq!(close.namespace.as_deref(), Some("icons"));
        assert_eq!(close.tags, ["ui"]);

        // Specificity merge: broad glob -> specific glob -> exact entry.
        let home = catalog.get("@app/icons/nav/home.svg").unwrap();
        assert_eq!(home.kind, AssetKind::Icon);
        assert_eq!(home.namespace.as_deref(), Some("icons/nav"));
        assert_eq!(home.title.as_deref(), Some("Home"));
        assert_eq!(home.description.as_deref(), Some("Navigation icon"));
        assert_eq!(home.tags, ["ui", "nav", "primary"]);
        assert_eq!(
            home.custom,
            json!({ "base": true, "scope": "nav", "weight": 2, "exact": true })
        );

        // Raster icon folds `@2x`.
        let star = catalog.get("@app/icons/raster/star.png").unwrap();
        assert_eq!(star.kind, AssetKind::Icon);
        assert_eq!(star.scales, [1.0, 2.0]);
        assert!(catalog.get("@app/icons/raster/star@2x.png").is_none());

        // `when` and `alias` stored and resolvable.
        let logo = catalog.get("brand/logo.svg").unwrap();
        assert_eq!(logo.path, "brand/acme/logo.svg");
        assert_eq!(logo.alias.as_deref(), Some("brand/logo.svg"));
        assert_eq!(
            logo.when,
            BTreeMap::from([
                ("brand".to_owned(), vec!["acme".to_owned()]),
                (
                    "platform".to_owned(),
                    vec!["ios".to_owned(), "macos".to_owned()]
                ),
            ])
        );

        // Catalog namespaces include explicit empty namespaces and derived entry namespaces.
        assert_eq!(
            catalog.namespaces(),
            &BTreeSet::from([
                "brand".to_owned(),
                "icons".to_owned(),
                "icons/custom".to_owned(),
                "icons/nav".to_owned(),
                "icons/raster".to_owned(),
                "illustrations/empty".to_owned(),
            ])
        );

        // Namespace queries: exact vs recursive.
        let exact_icons = catalog.query(&AssetQuery {
            namespace: Some("icons".to_owned()),
            namespace_recursive: false,
            ..AssetQuery::default()
        });
        assert_eq!(exact_icons.len(), 1);
        assert_eq!(exact_icons[0].path, "icons/close.svg");

        let recursive_icons = catalog.query(&AssetQuery {
            namespace: Some("icons".to_owned()),
            namespace_recursive: true,
            ..AssetQuery::default()
        });
        assert_eq!(recursive_icons.len(), 4);
    }

    #[test]
    fn assets_catalog_diagnostics_cover_zero_match_globs_extensions_and_svg_scales() {
        let manifest = AssetManifest::parse(
            r#"
[[asset]]
path = "missing/**/*.svg"
kind = "icon"

[[asset]]
path = "bad/font.png"
kind = "font"

[[asset]]
path = "icons/home.svg"
kind = "icon"
scales = [2]

[[asset]]
path = "dup.png"

[[asset]]
path = "dup.png"
"#,
        )
        .unwrap();

        let mut builder = AssetCatalog::builder();
        builder.add_manifest_with_files("app", &manifest, &["icons/home.svg".to_owned()]);
        let catalog = builder.build();

        use AssetDiagnosticSeverity::{Error, Warning};
        let diags: Vec<_> = catalog
            .diagnostics()
            .iter()
            .map(|d| {
                (
                    d.severity,
                    d.path.clone().unwrap_or_default(),
                    d.message.clone(),
                )
            })
            .collect();
        assert!(
            diags.iter().any(|(s, p, m)| *s == Warning
                && p == "missing/**/*.svg"
                && m.contains("no files")),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|(s, p, _)| *s == Error && p == "bad/font.png"),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|(s, p, _)| *s == Warning && p == "icons/home.svg"),
            "{diags:?}"
        );
        assert!(
            diags.iter().any(|(s, p, _)| *s == Error && p == "dup.png"),
            "{diags:?}"
        );
        // SVG icon scales were ignored and cleared.
        assert!(
            catalog
                .get("@app/icons/home.svg")
                .unwrap()
                .scales
                .is_empty()
        );
    }
}
