//! Real Workload Evidence (RWE) corpus contract and provider-free runner.
//!
//! Live multi-task RWE requires a separately persisted store-owned spend authorization.
//! Fixture completion is never labeled as a live baseline.

pub mod corpus;
pub mod runner;

pub use corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RWE_CORPUS_SCHEMA};
pub use runner::{
    evaluate_rwe_live_gate_from_store, persist_rwe_run_authorization,
    provider_free_rwe_readiness_dossier, run_provider_free_rwe, RweLiveGateResult,
    RweRunAuthorizationBody, RWE_RUN_AUTH_SCHEMA,
};
