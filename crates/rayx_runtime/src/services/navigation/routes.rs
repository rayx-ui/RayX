use super::{NavigationError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use url::Url;

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteDefinition {
    pub pattern: String,
    pub component: Option<String>,
    pub prewarm: bool,
}

impl RouteDefinition {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            component: None,
            prewarm: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteDescriptor {
    pub pattern: String,
    pub base_uri: String,
    pub path: String,
    pub segments: Vec<String>,
    pub order: usize,
    pub precedence: i32,
    pub is_catch_all: bool,
    pub component: Option<String>,
    pub prewarm: bool,
}

impl RouteDescriptor {
    pub fn new(
        definition: RouteDefinition,
        base_uri: impl Into<String>,
        order: usize,
    ) -> Result<Self> {
        let base_uri = base_uri.into();
        let route = Route::parse(&definition.pattern, &base_uri, None)?;
        let is_catch_all = route
            .segments
            .iter()
            .any(|segment| is_catch_all_parameter(segment));
        validate_pattern(&definition.pattern, &route.segments)?;

        let precedence = calculate_precedence(&route.segments, is_catch_all);
        Ok(Self {
            pattern: definition.pattern,
            base_uri,
            path: route.path,
            segments: route.segments,
            order,
            precedence,
            is_catch_all,
            component: definition.component,
            prewarm: definition.prewarm,
        })
    }

    pub fn matches(&self, route: &Route) -> Option<RouteParams> {
        if self.is_catch_all {
            match_catch_all_route(&self.segments, &route.segments)
        } else {
            match_route(&self.segments, &route.segments)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub uri: String,
    pub base_uri: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
    pub location: String,
    pub segments: Vec<String>,
    pub params: RouteParams,
    pub query_params: RouteParams,
    pub passed_parameters: Option<Value>,
    pub component: Option<String>,
    pub pattern: String,
    pub prewarm: bool,
}

impl Route {
    pub fn parse(
        uri: impl AsRef<str>,
        base_uri: impl AsRef<str>,
        passed_parameters: Option<Value>,
    ) -> Result<Self> {
        let base_uri = sanitize_base_uri(base_uri.as_ref());
        let uri = sanitize_uri(uri.as_ref());
        let parts = route_parts(&uri, &base_uri)?;
        let location = build_location(&parts.path, &parts.query, &parts.fragment);

        Ok(Self {
            uri,
            base_uri,
            path: parts.path.clone(),
            query: parts.query.clone(),
            fragment: parts.fragment.clone(),
            location,
            segments: split_segments(&parts.path),
            params: RouteParams::default(),
            query_params: parse_query_params(&parts.query),
            passed_parameters,
            component: None,
            pattern: String::new(),
            prewarm: false,
        })
    }

    pub(crate) fn with_descriptor(
        mut self,
        descriptor: &RouteDescriptor,
        params: RouteParams,
    ) -> Self {
        self.params = params;
        self.component = descriptor.component.clone();
        self.pattern = descriptor.pattern.clone();
        self.prewarm = descriptor.prewarm;
        self
    }
}

#[derive(Clone, Debug)]
pub struct RouteMap {
    base_uri: String,
    routes: Vec<RouteDefinition>,
    descriptors: Vec<RouteDescriptor>,
}

impl RouteMap {
    pub fn new(base_uri: impl Into<String>) -> Self {
        Self {
            base_uri: sanitize_base_uri(&base_uri.into()),
            routes: Vec::new(),
            descriptors: Vec::new(),
        }
    }

    pub fn route(&mut self, pattern: impl Into<String>) -> RouteBuilder<'_> {
        RouteBuilder {
            map: self,
            definition: RouteDefinition {
                pattern: pattern.into(),
                component: None,
                prewarm: false,
            },
        }
    }

    pub fn try_add(&mut self, definition: RouteDefinition) -> Result<()> {
        if self
            .routes
            .iter()
            .any(|route| route.pattern == definition.pattern)
        {
            return Err(NavigationError::DuplicateRoute(definition.pattern));
        }

        let descriptor =
            RouteDescriptor::new(definition.clone(), self.base_uri.clone(), self.routes.len())?;
        self.routes.push(definition);
        self.descriptors.push(descriptor);
        self.descriptors.sort_by(|left, right| {
            right
                .precedence
                .cmp(&left.precedence)
                .then_with(|| left.order.cmp(&right.order))
        });
        Ok(())
    }

    pub fn add(&mut self, definition: RouteDefinition) {
        if let Err(error) = self.try_add(definition) {
            panic!("route definition should be valid and unique: {error}");
        }
    }

    pub fn process_route(
        &self,
        uri: impl AsRef<str>,
        passed_parameters: Option<Value>,
    ) -> Result<RoutingData> {
        let uri = uri.as_ref();
        if is_external_uri(uri, &self.base_uri) {
            return Ok(RoutingData {
                status: RoutingStatus::SkippedExternalRoute,
                route: Route::parse(uri, &self.base_uri, passed_parameters)?,
            });
        }

        let route = Route::parse(uri, &self.base_uri, passed_parameters)?;
        for descriptor in &self.descriptors {
            if let Some(params) = descriptor.matches(&route) {
                return Ok(RoutingData {
                    status: RoutingStatus::Success,
                    route: route.with_descriptor(descriptor, params),
                });
            }
        }

        Ok(RoutingData {
            status: RoutingStatus::FailedNotFound,
            route,
        })
    }

    pub fn descriptors(&self) -> &[RouteDescriptor] {
        &self.descriptors
    }

    pub fn routes(&self) -> &[RouteDefinition] {
        &self.routes
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn base_uri(&self) -> &str {
        &self.base_uri
    }
}

impl Default for RouteMap {
    fn default() -> Self {
        Self::new("app://")
    }
}

pub struct RouteBuilder<'a> {
    map: &'a mut RouteMap,
    definition: RouteDefinition,
}

impl RouteBuilder<'_> {
    pub fn component(mut self, component: impl Into<String>) -> Self {
        self.definition.component = Some(component.into());
        self
    }

    pub fn prewarmed(mut self) -> Self {
        self.definition.prewarm = true;
        self
    }

    pub fn finish(self) {
        self.map.add(self.definition);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingData {
    pub status: RoutingStatus,
    pub route: Route,
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ParsedRouteParts {
    path: String,
    query: String,
    fragment: String,
}

fn validate_pattern(pattern: &str, segments: &[String]) -> Result<()> {
    let mut names = HashSet::new();
    for (index, segment) in segments.iter().enumerate() {
        let Some(name) = route_parameter_name(segment) else {
            continue;
        };

        if name.is_empty() {
            return Err(NavigationError::InvalidRoutePattern {
                pattern: pattern.to_string(),
                message: "route parameters must have names".to_string(),
            });
        }

        if is_catch_all_parameter(segment) && index != segments.len() - 1 {
            return Err(NavigationError::InvalidRoutePattern {
                pattern: pattern.to_string(),
                message: "catch-all route parameter must be the final segment".to_string(),
            });
        }

        if !names.insert(name.to_string()) {
            return Err(NavigationError::InvalidRoutePattern {
                pattern: pattern.to_string(),
                message: format!("duplicate route parameter `{name}`"),
            });
        }
    }

    Ok(())
}

fn match_route(template_segments: &[String], path_segments: &[String]) -> Option<RouteParams> {
    if template_segments.len() != path_segments.len() {
        return None;
    }

    let mut params = BTreeMap::new();
    for (template, path) in template_segments.iter().zip(path_segments.iter()) {
        if let Some(name) = route_parameter_name(template) {
            params.insert(name.to_string(), path.to_string());
        } else if !template.eq_ignore_ascii_case(path) {
            return None;
        }
    }

    Some(RouteParams(params))
}

fn match_catch_all_route(
    template_segments: &[String],
    path_segments: &[String],
) -> Option<RouteParams> {
    let mut params = BTreeMap::new();
    for (index, template) in template_segments.iter().enumerate() {
        if is_catch_all_parameter(template) {
            let name = route_parameter_name(template)?;
            params.insert(name.to_string(), path_segments[index..].join("/"));
            return Some(RouteParams(params));
        }

        let path = path_segments.get(index)?;
        if let Some(name) = route_parameter_name(template) {
            params.insert(name.to_string(), path.to_string());
        } else if !template.eq_ignore_ascii_case(path) {
            return None;
        }
    }

    (template_segments.len() == path_segments.len()).then_some(RouteParams(params))
}

fn route_parameter_name(segment: &str) -> Option<&str> {
    if is_catch_all_parameter(segment) {
        return segment.strip_prefix("{*")?.strip_suffix('}');
    }

    segment.strip_prefix('{')?.strip_suffix('}')
}

fn is_catch_all_parameter(segment: &str) -> bool {
    segment.starts_with("{*") && segment.ends_with('}')
}

fn calculate_precedence(segments: &[String], is_catch_all: bool) -> i32 {
    let mut score = if is_catch_all { -1000 } else { 0 };
    for segment in segments {
        if segment.is_empty() {
            continue;
        }

        if is_catch_all_parameter(segment) {
            score -= 100;
        } else if route_parameter_name(segment).is_some() {
            score += 10;
        } else {
            score += 100;
        }
    }

    score
}

fn route_parts(uri: &str, base_uri: &str) -> Result<ParsedRouteParts> {
    if is_absolute_uri(uri)
        && let Ok(url) = Url::parse(uri)
    {
        return Ok(ParsedRouteParts {
            path: sanitize_path(url.path()),
            query: url.query().unwrap_or_default().to_string(),
            fragment: url.fragment().unwrap_or_default().to_string(),
        });
    }

    let relative = uri
        .strip_prefix(base_uri.trim_end_matches('/'))
        .unwrap_or(uri)
        .trim_start_matches('/');
    let relative = if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{relative}")
    };

    let (without_fragment, fragment) = split_once(&relative, '#');
    let (path, query) = split_once(without_fragment, '?');
    Ok(ParsedRouteParts {
        path: sanitize_path(path),
        query: query.to_string(),
        fragment: fragment.to_string(),
    })
}

fn sanitize_uri(uri: &str) -> String {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }

    if is_absolute_uri(trimmed) {
        return trimmed.trim_end_matches('/').to_string();
    }

    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };

    sanitize_relative_uri(&prefixed)
}

fn sanitize_relative_uri(uri: &str) -> String {
    let (without_fragment, fragment) = split_once(uri, '#');
    let (path, query) = split_once(without_fragment, '?');
    build_location(&sanitize_path(path), query, fragment)
}

fn sanitize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };

    prefixed.trim_end_matches('/').to_string()
}

fn sanitize_base_uri(base_uri: &str) -> String {
    let trimmed = base_uri.trim();
    if trimmed.is_empty() {
        "app://".to_string()
    } else {
        trimmed.to_string()
    }
}

fn split_segments(path: &str) -> Vec<String> {
    path.split('/').map(ToOwned::to_owned).collect()
}

fn parse_query_params(query: &str) -> RouteParams {
    if query.is_empty() {
        return RouteParams::default();
    }

    RouteParams(
        query
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (key, value) = split_once(part, '=');
                (key.to_string(), value.to_string())
            })
            .collect(),
    )
}

fn build_location(path: &str, query: &str, fragment: &str) -> String {
    let mut location = path.to_string();
    if !query.is_empty() {
        location.push('?');
        location.push_str(query);
    }
    if !fragment.is_empty() {
        location.push('#');
        location.push_str(fragment);
    }
    location
}

fn split_once(value: &str, marker: char) -> (&str, &str) {
    match value.split_once(marker) {
        Some((left, right)) => (left, right),
        None => (value, ""),
    }
}

fn is_absolute_uri(uri: &str) -> bool {
    uri.contains("://") && Url::parse(uri).is_ok()
}

fn is_external_uri(uri: &str, base_uri: &str) -> bool {
    if !is_absolute_uri(uri) {
        return false;
    }

    let Ok(target) = Url::parse(uri) else {
        return false;
    };

    let Ok(base) = Url::parse(base_uri) else {
        return false;
    };

    target.scheme() != base.scheme()
        || target.host_str() != base.host_str()
        || target.port_or_known_default() != base.port_or_known_default()
        || !target.path().starts_with(base.path())
}
