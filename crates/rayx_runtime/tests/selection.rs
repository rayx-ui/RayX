//! The selection service: the active area's summary, its listeners and its reset on navigation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::{StartupInitializationContext, StartupInitializationError};
use rayx_app::services::navigation::NavigationService;
use rayx_app::testing::FakeNavigation;
use rayx_app::{AppBuilder as _, RayXAppBuilder, ServiceProvider};
use rayx_runtime::services::selection::{
    SelectionAppBuilderExt, SelectionAreaId, SelectionService, SelectionStatus,
};

fn selected(characters: usize) -> SelectionStatus {
    SelectionStatus {
        participant_count: 1,
        selected_character_count: characters,
        is_collapsed: false,
        is_selecting: false,
    }
}

/// `builder` with a fake navigation as its `dyn NavigationService`.
fn with_navigation(mut builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .services_mut()
        .replace_service::<dyn NavigationService>(
            FakeNavigation::new() as Arc<dyn NavigationService>
        );
    builder
}

fn services() -> (Arc<dyn NavigationService>, SelectionService) {
    let services = with_navigation(RayXAppBuilder::new())
        .add_selection()
        .build()
        .unwrap();
    (
        services
            .required_service::<dyn NavigationService>()
            .unwrap(),
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
    block_on(navigation.navigate_to("/b")).unwrap();
    assert_eq!(selection.active_area(), None);
    assert_eq!(selection.status(), SelectionStatus::default());
    assert_eq!(selection.navigation_revision(), revision + 1);
}

/// Builds the app and runs its startup, as the NSH layer does.
fn start(builder: RayXAppBuilder) -> Result<ServiceProvider, StartupInitializationError> {
    let services = builder.build().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )?;
    Ok(services)
}

#[test]
fn the_selection_chains_and_follows_a_navigation_registered_after_it() {
    let services = start(with_navigation(RayXAppBuilder::new().add_selection())).unwrap();
    let selection = services.required_service::<SelectionService>().unwrap();
    selection.activate(SelectionAreaId::from_raw(1), selected(5));
    block_on(
        services
            .required_service::<dyn NavigationService>()
            .unwrap()
            .navigate_to("/b"),
    )
    .unwrap();
    assert_eq!(selection.active_area(), None);
}

#[test]
fn startup_fails_without_the_navigation() {
    let Err(StartupInitializationError::ServiceFailures { failures }) =
        start(RayXAppBuilder::new().add_selection())
    else {
        panic!("startup should fail without a navigation");
    };
    let [failure] = failures.as_slice() else {
        panic!("one service should fail: {failures:?}");
    };
    assert!(failure.service.name().ends_with("SelectionService"));
    assert!(
        failure.message.contains("NavigationService"),
        "{}",
        failure.message
    );
}
