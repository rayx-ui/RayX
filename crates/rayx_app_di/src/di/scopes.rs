use super::provider::ServiceProvider;

#[derive(Clone, Debug)]
pub struct AppScope(pub ServiceProvider);

#[derive(Clone, Debug)]
pub struct WindowScope(pub ServiceProvider);

#[derive(Clone, Debug)]
pub struct ViewScope(pub ServiceProvider);
