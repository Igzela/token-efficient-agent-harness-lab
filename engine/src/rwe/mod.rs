//! Real Workload Evidence (RWE) corpus contract and provider-free runner prep.
//!
//! First RWE live execution requires a **separately persisted** operator spend
//! authorization envelope. This module never invents that authorization.

pub mod corpus;
pub mod runner;

pub use corpus::{freeze_first_rwe_corpus, FirstRweCorpus, RweTaskClass, RWE_CORPUS_SCHEMA};
pub use runner::{
    evaluate_rwe_live_gate, RweLiveGateResult, RweRunAuthorization, RWE_RUN_AUTH_SCHEMA,
};
