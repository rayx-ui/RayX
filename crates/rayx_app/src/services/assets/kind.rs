use serde::{Deserialize, Serialize};

/// What an asset is, from its manifest entry or its file name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// A bitmap image (`png`, `jpg`, `jpeg`, `gif`, `webp`, `avif`, `heic`).
    Image,
    /// An SVG image.
    Svg,
    /// A font (`ttf`, `otf`, `woff`, `woff2`).
    Font,
    /// Design tokens or a theme (JSON under `themes/`, or `*.dtcg.json`).
    Theme,
    /// Translations (JSON or TOML under `locales/`).
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
        if matches!(extension, Some("json" | "toml")) {
            if has_segment(&path, "locales") {
                return Self::Locale;
            }
            if has_segment(&path, "themes") || path.ends_with(".dtcg.json") {
                return Self::Theme;
            }
            if has_segment(&path, "fixtures") {
                return Self::Fixture;
            }
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

    /// Whether an image of this kind ships density variants (`@2x`, `@3x`): bitmaps do, SVG
    /// images draw sharp at any density.
    pub fn has_density_variants(self) -> bool {
        self == Self::Image
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
