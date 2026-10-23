pub trait DiReady: Send + Sync + 'static {}

impl<T> DiReady for T where T: ?Sized + Send + Sync + 'static {}

pub use DiReady as DIReady;
