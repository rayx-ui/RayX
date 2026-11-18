//! The platform's part of the navigation: where the app's location is shown and comes from.

use rayx_app::services::navigation::{NavigationHistoryMode, Result};

/// Where the app's location is shown and comes from: the browser's address bar, an embedding
/// app, a deep link. The location is the URL region's route (behind the base path) with the
/// other regions' routes in its query: `/projects/12?panel=%2Fsettings%2Fprofile`. RayX's on NSH
/// is the crate's `NshLocationSynchronizer`; tests give their own
/// (`testing::add_navigation_with`).
pub trait NavigationLocationSynchronizer: Send + Sync + 'static {
    /// The platform's location as the app starts (a deep link, the address bar), if it has one.
    fn location(&self) -> Option<String> {
        None
    }

    /// Shows `location` after a navigation, as a new entry of the platform's history (`Push`)
    /// or in place of the current one (the other modes).
    fn synchronize(&self, location: &str, history_mode: NavigationHistoryMode) -> Result<()>;
}

/// Shows nothing and starts nowhere.
#[derive(Clone, Debug, Default)]
pub struct NoopNavigationLocationSynchronizer;

impl NavigationLocationSynchronizer for NoopNavigationLocationSynchronizer {
    fn synchronize(&self, _location: &str, _history_mode: NavigationHistoryMode) -> Result<()> {
        Ok(())
    }
}
