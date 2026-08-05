//! Real Workload Evidence (RWE) corpus contract and provider-free runner.
//!
//! Live multi-task RWE requires a separately persisted store-owned spend authorization.
//! Fixture completion is never labeled as a live baseline.

pub mod corpus;
pub mod economic_protocol;
pub mod execution_schedule;
pub mod operator_corpus;
pub mod runner;

pub use corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RWE_CORPUS_SCHEMA};
pub use economic_protocol::{
    classify_evidence_sufficiency, freeze_rwe_economic_protocol, freeze_vde_artifact,
    EvidenceSufficiency, FrozenEvidenceDocument, IMPLEMENTATION_COST_RECEIPT_SCHEMA,
    RWE_ECONOMIC_PROTOCOL_SCHEMA, TASK_VALUE_PROFILE_SCHEMA, VERIFIED_DELIVERY_COMPARISON_SCHEMA,
    VERIFIED_DELIVERY_OBSERVATION_SCHEMA,
};
pub use execution_schedule::{freeze_operator_execution_schedule, FrozenExecutionSchedule};
pub use operator_corpus::{freeze_operator_rwe_corpus, operator_corpus_root};
pub use runner::{
    evaluate_rwe_live_gate_from_store, persist_rwe_run_authorization,
    provider_free_rwe_readiness_dossier, run_provider_free_rwe, RweLiveGateResult,
    RweRunAuthorizationBody, RWE_RUN_AUTH_SCHEMA,
};
