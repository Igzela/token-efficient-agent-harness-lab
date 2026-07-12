pub mod adaptive_auto_promotion;
pub mod adaptive_candidate;
pub mod adaptive_experiment;
pub mod adaptive_fusion;
pub mod auto_adjustment_guard;
pub mod auto_adjustment_policy;
pub mod contextual_policy;
pub mod endpoint_registry;
pub mod offline_evaluation;
pub mod outcome_attributor;
pub mod pattern_detector;
pub mod policy_proposer;
pub mod policy_simulator;
pub mod policy_snapshot;
pub mod proposal_serializer;
pub mod proposal_validator;
pub mod replay_eligibility;
pub mod run_trace_recorder;
pub mod shadow_router;

pub use adaptive_auto_promotion::{
    AdaptiveAutoPromotionController, AdaptiveAutoPromotionEvidence, AdaptiveAutoPromotionGate,
    AdaptiveAutoPromotionPolicy, AdaptiveAutoPromotionRequest,
};
pub use adaptive_candidate::CandidateKind as AdaptiveCandidateKind;
pub use adaptive_candidate::{
    AdaptiveCandidate, AdaptiveCandidateConfig, AdaptiveCandidateGenerator, AdaptiveCandidateSet,
    CandidateEndpointBinding, CandidateGenerationRequest, EndpointRejection, FusionRole,
    ADAPTIVE_CANDIDATE_SCHEMA_VERSION, ADAPTIVE_CANDIDATE_SET_SCHEMA_VERSION,
};
pub use adaptive_experiment::{
    AdaptiveExperimentController, AdaptiveExperimentDecision, AdaptiveExperimentError,
    AdaptiveExperimentGate, AdaptiveExperimentLimits, AdaptiveExperimentPolicy,
    AdaptiveExperimentRequest, ADAPTIVE_EXPERIMENT_SCHEMA_VERSION,
};
pub use adaptive_fusion::{
    AdaptiveFusionPlan, AdaptiveFusionPlanner, DeliberationMode, EndpointScorecard,
    ModelEndpointObservation, ObjectiveProfile, ObjectiveWeights, PortfolioRequest,
    ADAPTIVE_FUSION_PLAN_SCHEMA_VERSION,
};
pub use auto_adjustment_guard::{
    AutoAdjustmentGuard, AutoAdjustmentGuardDecision, AUTO_ADJUSTMENT_GUARD_DECISION_SCHEMA_VERSION,
};
pub use auto_adjustment_policy::{
    AutoAdjustmentEvidenceSummary, AutoAdjustmentPolicy, AutoAdjustmentPolicyDecision,
    AUTO_ADJUSTMENT_POLICY_DECISION_SCHEMA_VERSION, STRICT_AUTO_ADJUSTMENT_CONFIDENCE,
};
pub use contextual_policy::{
    contextual_policy_key, AdaptiveExplorationGate, ContextualBanditEngine,
    ContextualBanditObservation, ContextualPolicyDecision, ContextualPolicyError,
    ContextualPolicyPromotion, ContextualPolicyPromotionGate, ContextualPolicyPromotionVerdict,
    ContextualPolicyRequest, PromotedAdaptivePolicy, CONTEXTUAL_POLICY_DECISION_SCHEMA_VERSION,
    CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION, CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
pub use endpoint_registry::{
    CredentialReference, EndpointHealth, EndpointPricing, ModelEndpointRegistry,
    ModelEndpointRegistryError, ModelEndpointRegistrySnapshot, ModelEndpointSpec, RegistryMutation,
    ENDPOINT_REGISTRY_SCHEMA_VERSION,
};
pub use offline_evaluation::{
    offline_replay_report_sha256, CandidateAggregate, CandidateKind, JudgeCalibration,
    JudgeEvidence, OfflineCounterfactualEstimate, OfflineEvaluationEngine, OfflineEvaluationError,
    OfflineEvaluationReport, OfflineObservedFacts, OfflinePolicyComparison,
    OfflinePolicyDefinition, OfflinePolicySelection, OfflineReplayObservation,
    OfflineReplayOutcome, OfflineReplayReport, OfflineReplayRequest, OfflineReplayStatus,
    ShadowCandidateRecommendation, TaskClassEvaluation, OFFLINE_EVALUATION_SCHEMA_VERSION,
    OFFLINE_REPLAY_SCHEMA_VERSION,
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
pub use policy_snapshot::{
    snapshot_safety_hash, PolicySnapshotPreview, PolicySnapshotRecord,
    POLICY_SNAPSHOT_SCHEMA_VERSION,
};
pub use proposal_serializer::{
    serialize_candidate_to_api_response, serialize_candidate_to_proposal_request,
};
pub use proposal_validator::{ProposalValidator, ValidationResult};
pub use replay_eligibility::{
    evaluate_replay_eligibility, replay_eligibility_result_sha256,
    replay_observation_evidence_sha256, trace_content_sha256, CostEvidenceKind,
    EvidenceDisposition, JudgeCalibrationEvidence, JudgeReferenceEvidence,
    NormalizedReplayObservation, ReplayCandidateBinding, ReplayCohort, ReplayCoverage,
    ReplayEligibilityRequest, ReplayEligibilityResult, ReplayEnvelope, ReplayEvidenceError,
    ReplayEvidenceReference, ReplayEvidenceScope, ReplayMetricEnvelope, ReplayObservationEvidence,
    ReplayTraceInput, POLICY_REPLAY_CONTRACT_SCHEMA_VERSION, TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION,
};
pub use run_trace_recorder::{RunTrace, RunTraceRecorder, RUN_TRACE_SCHEMA_VERSION};
pub use shadow_router::{
    tier_cost_multiplier, ShadowDriftEvidence, ShadowPolicyComparison, ShadowReplayComparison,
    ShadowRouteOutput, ShadowRouter, SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION,
    SHADOW_ROUTE_SCHEMA_VERSION,
};
