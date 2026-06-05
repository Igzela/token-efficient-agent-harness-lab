#![allow(clippy::derivable_impls)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_map_or)]
#![allow(dead_code)]
#![recursion_limit = "256"]

pub mod app_layer;
pub mod budget_manager;
pub mod cli;
pub mod dispatch;
pub mod dispatch_decision;
pub mod dispatch_engine;
pub mod dispatch_ledger;
pub mod doc_generator;
pub mod ecosystem;
pub mod errors;
pub mod evaluation_stub;
pub mod event_schema;
pub mod event_source;
pub mod executor_adapter;
pub mod harness;
pub mod http_server;
pub mod infrastructure;
pub mod model_selector;
pub mod orchestration;
pub mod provider;
pub mod quality;
pub mod read_only_planner;
pub mod routing;
pub mod runtime;
pub mod sdk;
pub mod storage;
pub mod task_analyzer;
pub mod wire_types;
pub mod workflow;

pub use dispatch_engine::{build_dispatch_bundle, DispatchEngine};
