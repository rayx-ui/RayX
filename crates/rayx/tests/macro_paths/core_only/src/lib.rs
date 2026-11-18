//! `#[derive(AppService)]` in a crate on `rayx_app` alone names `::rayx_app::di::AppService`.

use rayx_app::AppService;

#[derive(AppService)]
pub struct Greeting(pub &'static str);

fn is_app_service<T: ::rayx_app::di::AppService>() {}

pub fn check() {
    is_app_service::<Greeting>();
}
