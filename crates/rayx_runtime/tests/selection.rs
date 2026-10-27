//! The selection service: the active area's summary, its listeners and its reset on navigation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rayx_core::RayXAppBuilder;
use rayx_runtime::services::navigation::{
    NavigationAppBuilderExt, NavigationService, RouteDefinition,
};
use rayx_runtime::services::selection::{
    SelectionAppBuilderExt, SelectionAreaId, SelectionService, SelectionStartupError,
    SelectionStatus,
};

fn selected(characters: usize) -> SelectionStatus {
    SelectionStatus {
        participant_count: 1,
        selected_character_count: characters,
        is_collapsed: false,
        is_selecting: false,
    }
}

fn services() -> (NavigationService, SelectionService) {
    let services = RayXAppBuilder::new()
        .add_navigation(|options| {
            options.add_route(RouteDefinition::new("/a"));
            options.add_route(RouteDefinition::new("/b"));
        })
        .add_selection()
        .unwrap()
        .build()
        .unwrap();
    (
        services
            .required_service::<NavigationService>()
            .unwrap()
            .as_ref()
            .clone(),
        services
            .required_service::<SelectionService>()
            .unwrap()
            .as_ref()
            .clone(),
    )
}

#[test]
fn status_is_content_free_and_reports_selection_presence() {
    assert!(selected(3).has_selection());
    assert!(!SelectionStatus::default().has_selection());
}

#[test]
fn only_the_active_area_publishes() {
    let (_, selection) = services();
    let first = SelectionAreaId::from_raw(1);
    let stale = SelectionAreaId::from_raw(2);
    selection.activate(first, selected(4));
    selection.publish_status(stale, SelectionStatus::default());
    assert_eq!(selection.active_area(), Some(first));
    assert_eq!(selection.status(), selected(4));

    let revision = selection.status_revision();
    selection.publish_status(first, selected(4));
    assert_eq!(selection.status_revision(), revision);
    selection.deactivate(stale);
    assert_eq!(selection.active_area(), Some(first));
    selection.deactivate(first);
    assert_eq!(selection.active_area(), None);
}

#[test]
fn listeners_hear_changes_until_dropped() {
    let (_, selection) = services();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let subscription =
        selection.subscribe(move |area, status| sink.lock().unwrap().push((area, status)));
    let area = SelectionAreaId::from_raw(7);
    selection.activate(area, selected(2));
    selection.activate(area, selected(2));
    drop(subscription);
    selection.clear();
    assert_eq!(*heard.lock().unwrap(), vec![(Some(area), selected(2))]);
}

#[test]
fn navigation_clears_the_selection() {
    let (navigation, selection) = services();
    selection.activate(SelectionAreaId::from_raw(1), selected(5));
    let revision = selection.navigation_revision();
    navigation.navigate_to("/b").unwrap();
    assert_eq!(selection.active_area(), None);
    assert_eq!(selection.status(), SelectionStatus::default());
    assert_eq!(selection.navigation_revision(), revision + 1);
}

#[test]
fn add_selection_needs_the_navigation() {
    assert_eq!(
        RayXAppBuilder::new().add_selection().err(),
        Some(SelectionStartupError::MissingNavigation)
    );
}
