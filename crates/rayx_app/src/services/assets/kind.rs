use serde::{Deserialize, Serialize};

/// What an asset is, from its manifest entry or its file name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// A monochrome or duotone icon (`svg`, `png`, `webp`).
    Icon,
    /// A multicolored illustration (`svg`, `png`, `webp`).
    Illustration,
    /// A keyboard shortcut map (`json`).
    Keymap,
    /// A bitmap image (`png`, `jpg`, `jpeg`, `gif`, `webp`, `avif`, `heic`).
    Image,
    /// An SVG image.
    Svg,
    /// A font (`ttf`, `otf`, `woff`, `woff2`).
    Font,
    /// Design tokens or a theme (mini-DTCG base theme or layered fragment JSON under `themes/`,
    /// or `*.dtcg.json`).
    Theme,
    /// Translations (JSON under `locales/` or `i18n/`).
    Locale,
    /// Test or sample data (JSON or TOML under `fixtures/`).
    Fixture,
    Markdown,
    Text,
    Json,
    Audio,
    Video,
    Binary,
    Unknown,
}

impl AssetKind {
    /// The kind a file's path suggests.
    pub fn infer_from_path(path: &str) -> Self {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let extension = extension(&path);
        if matches!(extension, Some("svg" | "png" | "webp")) {
            if has_segment(&path, "icons") {
                return Self::Icon;
            }
            if has_segment(&path, "illustrations") {
                return Self::Illustration;
            }
        }
        if matches!(extension, Some("json")) {
            if has_segment(&path, "locales") || has_segment(&path, "i18n") {
                return Self::Locale;
            }
            if has_segment(&path, "keymaps") || has_segment(&path, "keybindings") {
                return Self::Keymap;
            }
            if has_segment(&path, "themes") || path.ends_with(".dtcg.json") {
                return Self::Theme;
            }
            if has_segment(&path, "fixtures") {
                return Self::Fixture;
            }
        }
        if extension == Some("toml") && has_segment(&path, "fixtures") {
            return Self::Fixture;
        }
        match extension {
            Some("svg") => Self::Svg,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "heic") => Self::Image,
            Some("ttf" | "otf" | "woff" | "woff2") => Self::Font,
            Some("json") => Self::Json,
            Some("md" | "markdown") => Self::Markdown,
            Some("mp3" | "ogg" | "wav" | "flac" | "m4a") => Self::Audio,
            Some("mp4" | "webm" | "mov") => Self::Video,
            Some("txt" | "toml" | "yaml" | "yml" | "css" | "html" | "js" | "ts") => Self::Text,
            Some("bin" | "dat" | "wasm") => Self::Binary,
            _ => Self::Unknown,
        }
    }

    /// Whether assets of this kind can ship density variants (`@2x`, `@3x`).
    ///
    /// True for [`AssetKind::Image`], [`AssetKind::Icon`], and [`AssetKind::Illustration`].
    /// Use [`AssetKind::has_density_variants_for_path`] when the file path is known so SVG icons
    /// and illustrations do not expect raster `@2x` variants while `png`/`webp` ones do.
    pub fn has_density_variants(self) -> bool {
        matches!(self, Self::Image | Self::Icon | Self::Illustration)
    }

    /// Whether an asset of `self` at `path` supports density variants (`@2x`, `@3x`):
    /// true for [`AssetKind::Image`], and for raster (`png`, `webp`) [`AssetKind::Icon`] or
    /// [`AssetKind::Illustration`]; false for SVG icons/illustrations and all other kinds.
    pub fn has_density_variants_for_path(self, path: &str) -> bool {
        match self {
            Self::Image => true,
            Self::Icon | Self::Illustration => {
                let lower = path.replace('\\', "/").to_ascii_lowercase();
                matches!(extension(&lower), Some("png" | "webp"))
            }
            _ => false,
        }
    }

    /// Validates that `path`'s file extension is allowed for `self`.
    ///
    /// - `Font`: `ttf`, `otf`, `woff`, `woff2`
    /// - `Icon` / `Illustration`: `svg`, `png`, `webp`
    /// - `Locale` / `Keymap` / `Theme`: `json`
    /// - Other kinds: any extension is accepted.
    pub fn validate_extension(self, path: &str) -> Result<(), String> {
        let lower = path.replace('\\', "/").to_ascii_lowercase();
        let ext = extension(&lower);
        let allowed: Option<&[&str]> = match self {
            Self::Font => Some(&["ttf", "otf", "woff", "woff2"]),
            Self::Icon | Self::Illustration => Some(&["svg", "png", "webp"]),
            Self::Locale | Self::Keymap | Self::Theme => Some(&["json"]),
            _ => None,
        };
        let Some(allowed) = allowed else {
            return Ok(());
        };
        if ext.is_some_and(|e| allowed.contains(&e)) {
            Ok(())
        } else {
            let found = ext.map_or_else(|| "no extension".to_owned(), |e| format!("`.{e}`"));
            let expected = allowed
                .iter()
                .map(|e| format!("`.{e}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "{self:?} asset `{path}` has {found}; expected one of {expected}"
            ))
        }
    }

    /// The MIME type a file's path suggests.
    pub fn mime_type_for_path(path: &str) -> Option<&'static str> {
        let path = path.to_ascii_lowercase();
        Some(match extension(&path)? {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "avif" => "image/avif",
            "heic" => "image/heic",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "json" => "application/json",
            "md" | "markdown" => "text/markdown",
            "mp3" => "audio/mpeg",
            "ogg" => "audio/ogg",
            "wav" => "audio/wav",
            "flac" => "audio/flac",
            "m4a" => "audio/mp4",
            "mp4" => "video/mp4",
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            "txt" => "text/plain",
            "toml" => "application/toml",
            "yaml" | "yml" => "application/yaml",
            "css" => "text/css",
            "html" => "text/html",
            "js" => "text/javascript",
            "ts" => "text/typescript",
            "bin" | "dat" => "application/octet-stream",
            "wasm" => "application/wasm",
            _ => return None,
        })
    }
}

fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| !extension.is_empty())
}

fn has_segment(path: &str, expected: &str) -> bool {
    path.split('/').any(|segment| segment == expected)
}
