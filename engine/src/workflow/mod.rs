pub mod agent_profiles;
#[cfg(test)]
mod agent_profiles_tests;
pub mod backpressure;
pub mod checkpoint;
pub mod concurrency;
pub mod context_pack;
pub mod dag_manager;
pub mod dag_mutations;
pub mod dynamic_controller;
#[cfg(test)]
mod dynamic_controller_tests;
pub mod dynamic_decomposer;
#[cfg(test)]
mod dynamic_decomposer_tests;
#[cfg(test)]
mod dynamic_workflow_e2e_tests;
pub mod graph_operations;
pub mod orchestration_decision;
pub mod run_queue;
pub mod tool_registry;
#[cfg(test)]
mod tool_registry_tests;

// Re-exports for run_queue
pub use run_queue::{
    AdmissionResult, BackpressureSignal, DeadlineAction, QueueConfig, QueueStatus, QueuedRun,
    RunQueue, TenantQueueInfo,
};

// Re-exports for backpressure
pub use backpressure::{Backpressure, BackpressureConfig, BackpressureDecision, PauseAction};
