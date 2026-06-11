pub mod auto_adjustment_guard;
pub mod auto_adjustment_policy;
pub mod outcome_attributor;
pub mod pattern_detector;
pub mod policy_proposer;
pub mod policy_simulator;
pub mod policy_snapshot;
pub mod proposal_serializer;
pub mod proposal_validator;
pub mod run_trace_recorder;
pub mod shadow_router;

pub use auto_adjustment_guard::{
    AutoAdjustmentGuard, AutoAdjustmentGuardDecision, AUTO_ADJUSTMENT_GUARD_DECISION_SCHEMA_VERSION,
};
pub use auto_adjustment_policy::{
    AutoAdjustmentEvidenceSummary, AutoAdjustmentPolicy, AutoAdjustmentPolicyDecision,
    AUTO_ADJUSTMENT_POLICY_DECISION_SCHEMA_VERSION, STRICT_AUTO_ADJUSTMENT_CONFIDENCE,
};
pub use outcome_attributor::{OutcomeAttribution, OutcomeAttributor};
pub use pattern_detector::{DetectedPattern, PatternDetector};
pub use policy_proposer::{
    PolicyProposer, ProposalCandidate as GeneratedProposalCandidate,
    PROPOSAL_CANDIDATE_SCHEMA_VERSION,
};
pub use policy_simulator::{
    PolicyCandidate, PolicySimulator, SimulationResult, POLICY_SIMULATION_SCHEMA_VERSION,
};
pub use policy_snapshot::{PolicySnapshotPreview, POLICY_SNAPSHOT_SCHEMA_VERSION};
pub use proposal_serializer::{
    serialize_candidate_to_api_response, serialize_candidate_to_proposal_request,
};
pub use proposal_validator::{ProposalValidator, ValidationResult};
pub use run_trace_recorder::RunTraceRecorder;
pub use shadow_router::{
    tier_cost_multiplier, ShadowRouteOutput, ShadowRouter, SHADOW_ROUTE_SCHEMA_VERSION,
};
