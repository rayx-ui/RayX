//! Input maps: which action a key sequence or a pointer gesture runs in a context, in three
//! layers (framework, application, user), the user's stored in the app's key-value store. The
//! service resolves bindings and lists an action's shortcuts; the UI layer feeds it the
//! keystrokes and gestures it sees.

mod app_ext;
mod model;
mod options;
mod service;

pub use app_ext::InputMapAppBuilderExt;
pub use model::{
    INPUT_MAP_SCHEMA_VERSION, InputBinding, InputMapDocument, InputMapEntry, InputMapError,
    InputMapSource, InputModifiers, InputPointerButton, InputPointerGesture, InputWheelDirection,
    InputWheelSource, valid_key_sequence,
};
pub use options::{INPUT_MAP_OPTIONS_SECTION, InputMapOptions};
pub use service::{
    INPUT_MAP_STORAGE_BUCKET, InputMapApplyError, InputMapService, ResolvedInputAction,
};
