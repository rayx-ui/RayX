//! Asset paths: relative to a crate's `assets/` directory, with density suffixes.

use super::AssetError;

/// The density suffixes NSH loads (`@1.5x`, `@2x`, `@3x`, `@4x`), as scales.
pub const DENSITY_SCALES: [f32; 4] = [1.5, 2.0, 3.0, 4.0];

/// `path` checked and normalized: relative, `/`-separated, without `.` or `..` segments.
pub(crate) fn normalize(path: &str) -> Result<String, AssetError> {
    let invalid = |reason: &str| AssetError::InvalidPath {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(invalid("it is empty"));
    }
    if trimmed.contains('\\') {
        return Err(invalid("it uses `\\`; asset paths are `/`-separated"));
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err(invalid(
            "it is absolute; asset paths are relative to `assets/`",
        ));
    }
    let mut segments = Vec::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(invalid("it leaves `assets/`")),
            segment => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err(invalid("it names no file"));
    }
    Ok(segments.join("/"))
}

/// The base path and scale of a density variant's path (`logo@2x.png` is `logo.png` at 2),
/// for the suffixes NSH loads.
pub(crate) fn split_density(path: &str) -> Option<(String, f32)> {
    let (directory, name) = match path.rsplit_once('/') {
        Some((directory, name)) => (Some(directory), name),
        None => (None, path),
    };
    let (stem, extension) = name.rsplit_once('.')?;
    let (base_stem, suffix) = stem.rsplit_once('@')?;
    let scale: f32 = suffix.strip_suffix('x')?.parse().ok()?;
    if base_stem.is_empty() || !DENSITY_SCALES.contains(&scale) {
        return None;
    }
    let base_name = format!("{base_stem}.{extension}");
    Some((
        directory.map_or(base_name.clone(), |directory| {
            format!("{directory}/{base_name}")
        }),
        scale,
    ))
}

/// The variant path of `path` at `scale` (`logo.png` at 2 is `logo@2x.png`).
pub(crate) fn variant_path(path: &str, scale: f32) -> String {
    let suffix = if scale.fract() == 0.0 {
        format!("@{scale:.0}x")
    } else {
        format!("@{scale}x")
    };
    let (directory, name) = match path.rsplit_once('/') {
        Some((directory, name)) => (format!("{directory}/"), name),
        None => (String::new(), path),
    };
    match name.rsplit_once('.') {
        Some((stem, extension)) => format!("{directory}{stem}{suffix}.{extension}"),
        None => format!("{directory}{name}{suffix}"),
    }
}
