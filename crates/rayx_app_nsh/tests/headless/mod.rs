//! A headless TSX app on NSH's engine, on a thread of its own: `rayx_app_nsh::testing`'s harness
//! (`src/testing.rs`), included here so the host's own tests use it without the `testing`
//! feature.

#![allow(dead_code)]

include!("../../src/testing.rs");

pub mod greeting;
