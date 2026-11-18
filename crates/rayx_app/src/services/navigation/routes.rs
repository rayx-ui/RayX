use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A route's parameters (`{id}` of `/projects/{id}`) or its query's, by name.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteParams(BTreeMap<String, String>);

impl RouteParams {
    pub fn new(values: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        Self(
            values
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    pub fn into_inner(self) -> BTreeMap<String, String> {
        self.0
    }
}

/// A URI resolved against the navigation's routes: its parts, the route it matched (pattern,
/// component, parameters) and the parameters passed with the navigation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub uri: String,
    pub base_uri: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
    /// The path with its query and fragment (`/search?q=rayx#top`): what a journal entry and
    /// the app's location keep.
    pub location: String,
    pub segments: Vec<String>,
    pub params: RouteParams,
    pub query_params: RouteParams,
    pub passed_parameters: Option<Value>,
    pub component: Option<String>,
    pub pattern: String,
    pub prewarm: bool,
}

/// A route and how resolving it went.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingData {
    pub status: RoutingStatus,
    pub route: Route,
}

/// How a navigation went.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoutingStatus {
    None,
    Success,
    FailedNotFound,
    ComponentNotFound,
    SkippedSameRoute,
    SkippedExternalRoute,
    SkippedNoHistory,
    Cancelled,
}
