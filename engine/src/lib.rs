#![allow(clippy::derivable_impls)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_map_or)]
#![recursion_limit = "512"]

pub mod agent_memory;
pub mod budget_anomaly;
pub mod budget_forecast;
pub mod budget_manager;
pub mod cli;
pub mod dispatch_decision;
pub mod dispatch_engine;
pub mod dispatch_ledger;
pub mod doc_generator;
pub mod ecosystem;
pub mod efficiency_benchmark_runtime;
pub mod errors;
pub mod evaluation_stub;
pub mod event_schema;
pub(crate) mod event_source;
pub mod executor_adapter;
pub mod executor_pool;
pub mod external_runtime;
pub mod feedback;
pub(crate) mod harness;
pub mod harness_evolution;
pub mod harness_evolution_eval;
pub mod harness_evolution_pr_ready;
pub mod http_server;
pub mod infrastructure;
pub mod local_runner_provider;
pub mod local_scorecard_import;
pub mod model_selector;
pub mod node_executor;
pub mod opencode_runtime;
pub mod operator_decision;
pub mod orchestration;
pub mod provider;
pub mod quality;
pub mod read_only_planner;
pub mod recursive_execution;
pub mod routing;
pub mod runtime;
pub mod scheduler;
pub mod storage;
pub mod target_repo_output;
pub mod task_analyzer;
pub mod tool_policy_executor;
pub mod trusted_local;
pub mod wire_types;
pub mod workflow;

pub use dispatch_engine::{build_dispatch_bundle, DispatchEngine};
