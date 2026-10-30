//! diskgc-core — the engine behind `diskgc`.
//!
//! The crate is deliberately free of any UI or process-lifetime policy: the CLI, the daemon,
//! the IPC server and the tests all drive the same [`Engine`]. Everything that differs between
//! operating systems lives in [`platform`].
//!
//! Pipeline: [`rules`] turn the current state of the disk into a [`model::Plan`] (a list of
//! explained [`model::Action`]s); the [`executor`] applies a plan and measures what was really
//! freed; [`history`] records it; [`forecast`] and the daemon use the history to decide how
//! aggressive the next run should be.

pub mod bytes;
pub mod config;
pub mod daemon;
pub mod docker;
pub mod engine;
pub mod executor;
pub mod forecast;
pub mod fs;
pub mod history;
pub mod mem;
pub mod model;
pub mod paths;
pub mod platform;
pub mod protocol;
pub mod report;
pub mod rules;

pub use config::Config;
pub use engine::Engine;
pub use model::{Action, ActionKind, Level, Plan, RunSummary};
