use super::DiReady;

/// Marker for DI services (RayX's `RayXService`). Implement it with
/// `#[derive(AppService)]`, or for a trait object with
/// `impl rayx_core::di::AppService for dyn MyTrait {}`.
pub trait AppService: DiReady {}
