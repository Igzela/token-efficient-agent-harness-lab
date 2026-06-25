#![allow(dead_code)]

//! Reference-only module: append-only event sourcing patterns.
//!
//! Retained for architectural reference. The active runtime uses
//! `LocalProductStore` (SQLite CRUD) instead of append-only event stores.
//! Not wired into the dispatch kernel or HTTP API.

pub(crate) mod event_store;
pub mod project_board;
pub mod projection_store;
pub mod task_queue;
pub mod task_records;
pub mod validators;
