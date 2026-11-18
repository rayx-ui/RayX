//! RayX's selection service changed from many threads at once: areas activating, publishing and
//! deactivating on their threads while navigation clears the selection on others. Every change
//! reaches the published state: none overwrites another, and subscribers last hear it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use futures::executor::block_on;
use rayx_app::services::navigation::NavigationService;
use rayx_app::services::selection::{SelectionAreaId, SelectionService, SelectionStatus};
use rayx_app::testing::FakeNavigation;
use rayx_app::{AppBuilder as _, RayXAppBuilder};
use rayx_services_selection::SelectionAppBuilderExt as _;

const THREADS: usize = 4;
const ROUNDS: usize = 500;

fn selected(characters: usize) -> SelectionStatus {
    SelectionStatus {
        participant_count: 1,
        selected_character_count: characters,
        is_collapsed: false,
        is_selecting: false,
    }
}

#[test]
fn changes_and_navigations_from_many_threads_all_reach_the_published_state() {
    for _ in 0..5 {
        let mut builder = RayXAppBuilder::new();
        builder
            .services_mut()
            .replace_service::<dyn NavigationService>(
                FakeNavigation::new() as Arc<dyn NavigationService>
            );
        let services = builder.add_selection().build().unwrap();
        let navigation = services
            .required_service::<dyn NavigationService>()
            .unwrap();
        let selection = services.required_service::<dyn SelectionService>().unwrap();
        let last_heard = Arc::new(Mutex::new(None));
        let heard = Arc::clone(&last_heard);
        let _subscription = selection
            .selection()
            .subscribe(move |state| *heard.lock().unwrap() = Some(**state));

        let barrier = Arc::new(Barrier::new(THREADS * 2));
        let mut threads = Vec::new();
        for thread_index in 0..THREADS {
            // Navigation, each to a route of its own.
            let (navigation, start) = (Arc::clone(&navigation), Arc::clone(&barrier));
            threads.push(thread::spawn(move || {
                start.wait();
                for round in 0..ROUNDS {
                    block_on(navigation.navigate_to(format!("/t{thread_index}/{round}"))).unwrap();
                }
            }));
            // An area of its own, ending each round deactivated.
            let (selection, start) = (Arc::clone(&selection), Arc::clone(&barrier));
            threads.push(thread::spawn(move || {
                let area = SelectionAreaId::from_raw(thread_index as u64);
                start.wait();
                for round in 0..ROUNDS {
                    block_on(selection.activate(area, selected(round))).unwrap();
                    block_on(selection.publish_status(area, selected(round + 1))).unwrap();
                    block_on(selection.deactivate(area)).unwrap();
                }
                block_on(selection.clear()).unwrap();
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }

        let state = *selection.selection().get();
        assert_eq!(
            state.navigation_revision,
            (THREADS * ROUNDS) as u64,
            "a navigation was lost"
        );
        assert_eq!(state.active_area, None, "the last clear was lost");
        assert_eq!(state.status, SelectionStatus::default());
        assert_eq!(
            *last_heard.lock().unwrap(),
            Some(state),
            "subscribers last heard the published state"
        );
    }
}
