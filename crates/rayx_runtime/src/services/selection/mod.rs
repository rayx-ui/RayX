//! Selection: the app-wide summary of the active selection area (which area, how much is
//! selected), cleared when navigation changes the route. It holds no selected content; the
//! components that own a selection publish its summary.

mod app_ext;
mod selection_area_id;
mod selection_status;
mod service;

pub use app_ext::{SelectionAppBuilderExt, SelectionStartupError};
pub use selection_area_id::SelectionAreaId;
pub use selection_status::SelectionStatus;
pub use service::SelectionService;
