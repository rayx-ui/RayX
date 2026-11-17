//! App services: the app builder ([`app`]) and the core services' abstractions, one module per
//! service, each a `#[contract]` trait with the data types its members use. RayX's
//! implementations live in the `rayx_services_<name>` crates; an app may register its own, in
//! Rust or in TS.

pub mod app;
pub mod assets;
pub mod fonts;
pub mod history;
pub mod i18n;
pub mod input_map;
pub mod navigation;
pub mod operations;
pub mod platform;
pub mod selection;
pub mod storage;
pub mod suspension;

use std::fmt;

/// Why a call of a core service's contract failed before its implementation answered, or what
/// the implementation threw: what each service's error type converts from (`From<ContractFailure>`),
/// so a TS implementation's failures reach Rust in the service's own error type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractFailure {
    /// The contract's name.
    pub contract: &'static str,
    /// The member that failed.
    pub member: &'static str,
    /// Why.
    pub message: String,
}

impl fmt::Display for ContractFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}.{}`: {}", self.contract, self.member, self.message)
    }
}

impl std::error::Error for ContractFailure {}
