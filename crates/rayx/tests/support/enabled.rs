//! The service crates the umbrella's features enable in the test's build, independently of the
//! umbrella's own registration: their sets and their TS builder methods.

#![allow(dead_code)]

use rayx::prelude::Registry;

/// Each service's feature, its builder method in TS and whether the test's build enables it.
pub const BUILDER_METHODS: [(&str, &str, bool); 12] = [
    (
        "application_configuration",
        "addApplicationConfiguration",
        cfg!(feature = "application_configuration"),
    ),
    ("assets", "addAssets", cfg!(feature = "assets")),
    ("fonts", "addFonts", cfg!(feature = "fonts")),
    ("history", "addHistory", cfg!(feature = "history")),
    ("i18n", "addI18n", cfg!(feature = "i18n")),
    ("input_map", "addInputMap", cfg!(feature = "input_map")),
    ("navigation", "addNavigation", cfg!(feature = "navigation")),
    ("operations", "addOperations", cfg!(feature = "operations")),
    ("platform", "addPlatform", cfg!(feature = "platform")),
    ("selection", "addSelection", cfg!(feature = "selection")),
    ("storage", "addStorage", cfg!(feature = "storage")),
    ("suspension", "addSuspension", cfg!(feature = "suspension")),
];

/// Adds the set of every service crate the test's build enables, through `rayx::services`.
#[allow(unused_variables)]
pub fn register_service_sets(registry: &mut Registry) {
    #[cfg(feature = "application_configuration")]
    rayx::services::application_configuration::register(registry);
    #[cfg(feature = "assets")]
    rayx::services::assets::register(registry);
    #[cfg(feature = "fonts")]
    rayx::services::fonts::register(registry);
    #[cfg(feature = "history")]
    rayx::services::history::register(registry);
    #[cfg(feature = "i18n")]
    rayx::services::i18n::register(registry);
    #[cfg(feature = "input_map")]
    rayx::services::input_map::register(registry);
    #[cfg(feature = "navigation")]
    rayx::services::navigation::register(registry);
    #[cfg(feature = "operations")]
    rayx::services::operations::register(registry);
    #[cfg(feature = "platform")]
    rayx::services::platform::register(registry);
    #[cfg(feature = "selection")]
    rayx::services::selection::register(registry);
    #[cfg(feature = "storage")]
    rayx::services::storage::register(registry);
    #[cfg(feature = "suspension")]
    rayx::services::suspension::register(registry);
}
