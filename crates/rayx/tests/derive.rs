//! `#[derive(AppService)]` in a crate that reaches RayX through the umbrella alone: the derive
//! names the trait as `::rayx::core::di::AppService`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use rayx::prelude::*;

#[derive(AppService)]
struct Feed {
    label: &'static str,
}

#[derive(AppService)]
struct Cache<T: Send + Sync + 'static> {
    entries: Vec<T>,
}

fn is_app_service<T: AppService + ?Sized>() {}

#[test]
fn the_derive_works_through_the_umbrella() {
    is_app_service::<Feed>();
    is_app_service::<Cache<u32>>();
    let services = ServiceProvider::default()
        .with_singleton(Feed { label: "live" })
        .with_singleton(Cache::<u32> {
            entries: vec![1, 2],
        });
    assert_eq!(services.required_service::<Feed>().unwrap().label, "live");
    assert_eq!(
        services.required_service::<Cache<u32>>().unwrap().entries,
        [1, 2]
    );
}
