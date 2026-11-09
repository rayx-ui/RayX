use std::sync::{Arc, Mutex, PoisonError};

use rayx_core::di::AppService;

/// The files the app's packages ship, as the platform lists them (`asset:/@<package>/<path>`
/// URLs; `rayx_app` takes NSH's `assets!()`): the asset service catalogs them without a
/// manifest listing them, and reads a package's manifest only when the package ships one, for
/// the metadata it adds.
#[derive(Clone, Debug, Default, AppService)]
pub struct BundledAssets {
    files: Arc<Mutex<Vec<(String, String)>>>,
}

impl BundledAssets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the file at `uri` (`asset:/@<package>/<path>`); returns whether it is such a URL
    /// (one that is not is left out).
    pub fn add(&self, uri: &str) -> bool {
        let Some((package, path)) = uri
            .strip_prefix("asset:/@")
            .and_then(|rest| rest.split_once('/'))
            .filter(|(package, path)| !package.is_empty() && !path.is_empty())
        else {
            return false;
        };
        let file = (package.to_owned(), path.to_owned());
        let mut files = self.lock();
        if !files.contains(&file) {
            files.push(file);
        }
        true
    }

    /// The packages with files, in the order their first file was added.
    pub fn packages(&self) -> Vec<String> {
        let mut packages: Vec<String> = Vec::new();
        for (package, _) in self.lock().iter() {
            if !packages.contains(package) {
                packages.push(package.clone());
            }
        }
        packages
    }

    /// The paths of `package`'s files, in its `assets/` directory, in the order they were
    /// added.
    pub fn files(&self, package: &str) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(owner, _)| owner == package)
            .map(|(_, path)| path.clone())
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, String)>> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
