//! The selection service: the active area's summary, its listeners and its reset on navigation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use rayx_app::di::ServiceValidationProblem;
use rayx_app::services::navigation::NavigationService;
use rayx_app::services::selection::{
    SelectionAreaId, SelectionService, SelectionState, SelectionStatus,
};
use rayx_app::testing::{FakeNavigation, FakeSelection};
use rayx_app::{AppBuildError, AppBuilder as _, RayXAppBuilder, ServiceValidationError};
use rayx_services_selection::SelectionAppBuilderExt as _;

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

fn services() -> (Arc<dyn NavigationService>, Arc<dyn SelectionService>) {
    let services = with_navigation(RayXAppBuilder::new())
        .add_selection()
        .build()
        .unwrap();
    (
        services
            .required_service::<dyn NavigationService>()
            .unwrap(),
        services.required_service::<dyn SelectionService>().unwrap(),
    )
}

/// Runs a command of the service, which RayX's answers at once.
fn run<E: std::fmt::Debug>(command: futures::future::BoxFuture<'_, Result<(), E>>) {
    block_on(command).unwrap();
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
    run(selection.activate(first, selected(4)));
    run(selection.publish_status(stale, SelectionStatus::default()));
    assert_eq!(selection.active_area(), Some(first));
    assert_eq!(selection.status(), selected(4));

    let revision = selection.status_revision();
    run(selection.publish_status(first, selected(4)));
    assert_eq!(selection.status_revision(), revision);
    run(selection.publish_status(first, selected(6)));
    assert_eq!(selection.status_revision(), revision + 1);
    run(selection.deactivate(stale));
    assert_eq!(selection.active_area(), Some(first));
    run(selection.deactivate(first));
    assert_eq!(selection.active_area(), None);
}

#[test]
fn the_revisions_count_what_changed() {
    let (_, selection) = services();
    let (one, two) = (SelectionAreaId::from_raw(1), SelectionAreaId::from_raw(2));
    run(selection.activate(one, selected(1)));
    run(selection.activate(two, selected(1)));
    run(selection.clear());
    assert_eq!(
        *selection.selection().get(),
        SelectionState {
            active_area: None,
            status: SelectionStatus::default(),
            navigation_revision: 0,
            active_area_revision: 3,
            status_revision: 2,
        }
    );
}

#[test]
fn listeners_hear_changes_until_dropped() {
    let (_, selection) = services();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let subscription =
        selection.subscribe(move |area, status| sink.lock().unwrap().push((area, status)));
    let area = SelectionAreaId::from_raw(7);
    run(selection.activate(area, selected(2)));
    run(selection.activate(area, selected(2)));
    drop(subscription);
    run(selection.clear());
    assert_eq!(*heard.lock().unwrap(), vec![(Some(area), selected(2))]);
}

#[test]
fn the_state_is_published_only_when_it_changes() {
    let (_, selection) = services();
    let published = Arc::new(Mutex::new(0));
    let count = Arc::clone(&published);
    let _subscription = selection
        .selection()
        .subscribe(move |_| *count.lock().unwrap() += 1);
    let area = SelectionAreaId::from_raw(3);
    run(selection.activate(area, selected(2)));
    run(selection.activate(area, selected(2)));
    run(selection.publish_status(SelectionAreaId::from_raw(4), selected(9)));
    run(selection.deactivate(SelectionAreaId::from_raw(4)));
    run(selection.clear());
    run(selection.clear());
    assert_eq!(*published.lock().unwrap(), 2);
}

#[test]
fn navigation_clears_the_selection() {
    let (navigation, selection) = services();
    run(selection.activate(SelectionAreaId::from_raw(1), selected(5)));
    let revision = selection.navigation_revision();
    block_on(navigation.navigate_to("/b")).unwrap();
    assert_eq!(selection.active_area(), None);
    assert_eq!(selection.status(), SelectionStatus::default());
    assert_eq!(selection.navigation_revision(), revision + 1);
}

#[test]
fn the_selection_chains_and_follows_a_navigation_registered_after_it() {
    let services = with_navigation(RayXAppBuilder::new().add_selection())
        .build()
        .unwrap();
    let selection = services.required_service::<dyn SelectionService>().unwrap();
    run(selection.activate(SelectionAreaId::from_raw(1), selected(5)));
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
fn building_without_the_navigation_fails_with_its_hint() {
    let Err(AppBuildError::Services(ServiceValidationError { problems })) =
        RayXAppBuilder::new().add_selection().build()
    else {
        panic!("built without a navigation");
    };
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            ServiceValidationProblem::Missing { needed, hint, .. }
                if needed.contains("NavigationService") && hint.contains("add_navigation(..)")
        )),
        "{problems:?}"
    );
}

#[test]
fn a_later_add_selection_replaces_the_app_s_own_and_a_later_own_replaces_rayx_s() {
    let mut builder = with_navigation(RayXAppBuilder::new());
    builder
        .services_mut()
        .replace_service::<dyn SelectionService>(FakeSelection::new() as Arc<dyn SelectionService>);
    let services = builder.add_selection().build().unwrap();
    let selection = services.required_service::<dyn SelectionService>().unwrap();
    run(selection.activate(SelectionAreaId::from_raw(1), selected(1)));
    // RayX's counts what changed; the fake leaves the revisions to its test.
    assert_eq!(selection.active_area_revision(), 1);

    let fake = FakeSelection::new();
    let mut builder = with_navigation(RayXAppBuilder::new()).add_selection();
    builder
        .services_mut()
        .replace_service::<dyn SelectionService>(Arc::clone(&fake) as Arc<dyn SelectionService>);
    let services = builder.build().unwrap();
    let selection = services.required_service::<dyn SelectionService>().unwrap();
    run(selection.clear());
    assert_eq!(fake.calls().len(), 1);
}
