//! RayX's navigation options.

use rayx_app::services::navigation::NavigationHistoryMode;
use serde::{Deserialize, Serialize};

use crate::routes::RouteDefinition;

/// The configuration section [`NavigationOptions`] bind from.
pub const NAVIGATION_OPTIONS_SECTION: &str = "RayX:Services:Navigation";

/// How RayX's navigation service routes and shows the app's location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NavigationOptions {
    pub base_uri: String,
    pub default_route: String,
    pub default_history_mode: NavigationHistoryMode,
    pub root_region_name: String,
    pub max_journal_entries: usize,
    pub synchronize_location: bool,
    /// The region whose route is the location's path (default: the root region).
    pub url_region: Option<String>,
    /// The location's prefix before the app's routes (`/app`; default none): the location of
    /// `/projects/12` is `/app/projects/12`.
    pub base_path: String,
    /// The other regions the location carries in its query, by name (`?panel=%2Fsettings`), so a
    /// location read at startup reaches them before they exist; a region made at run time is
    /// carried too.
    pub query_regions: Vec<String>,
    pub not_found_route: Option<String>,
    pub external_route: Option<String>,
    pub component_not_found_route: Option<String>,
    pub routes: Vec<RouteDefinition>,
}

impl Default for NavigationOptions {
    fn default() -> Self {
        Self {
            base_uri: "app://".to_string(),
            default_route: "/".to_string(),
            default_history_mode: NavigationHistoryMode::Push,
            root_region_name: "root".to_string(),
            max_journal_entries: 40,
            synchronize_location: true,
            url_region: None,
            base_path: String::new(),
            query_regions: Vec::new(),
            not_found_route: None,
            external_route: None,
            component_not_found_route: None,
            routes: Vec::new(),
        }
    }
}

impl NavigationOptions {
    pub fn add_route(&mut self, definition: RouteDefinition) -> &mut Self {
        self.routes.push(definition);
        self
    }
}
