pub(crate) mod handlers;
pub(crate) mod middleware;
pub(crate) mod routes;
pub(crate) mod server_context;
pub(crate) mod state;

pub use routes::{build_axum_router, build_axum_router_with_dashboard};
pub use server_context::{RouteHandler, RouteMatch, ServerContext};
pub use state::{AxumApiState, CliCapability, ServerConfig};

pub const HTTP_SERVER_SCHEMA_VERSION: &str = "http_server.v1";
pub const AXUM_API_SCHEMA_VERSION: &str = "axum_api.v1";
pub const MAX_BODY_SIZE: usize = 1_048_576;

use crate::feedback::{ContextualPolicyPromotion, ObjectiveProfile};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DispatchApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AdaptiveFusionCompletionApiRequest {
    pub prompt: String,
    pub task_class: Option<String>,
    pub objective: Option<ObjectiveProfile>,
    pub risk_level: Option<String>,
    pub metadata: Option<Value>,
    pub include_routing_metadata: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ProviderEndpointConfigApiRequest {
    pub endpoints: Vec<crate::provider::adaptive_execution::AdaptiveProviderEndpointConfig>,
    pub confirm_provider_endpoint_config: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStepPlanApiRequest {
    pub agent_id: String,
    pub role: String,
    pub capability_profile: Vec<String>,
    pub profile_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyPlanApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
    pub adaptive_execution: Option<Value>,
    pub confirm_adaptive_execution_plan: Option<bool>,
    pub agent_steps: Option<Vec<AgentStepPlanApiRequest>>,
    pub confirm_agent_runtime_plan: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunCreateApiRequest {
    pub plan_id: String,
    pub confirm_execution: Option<bool>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunEventApiRequest {
    pub node_id: Option<String>,
    pub event_type: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunApprovalApiRequest {
    pub node_id: String,
    pub decision: String,
    pub reason: Option<String>,
    pub bound_patch_hash: Option<String>,
    pub bound_source_revision: Option<String>,
    pub bound_changed_files: Option<Vec<String>>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunActionApiRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunTickApiRequest {
    pub actor: Option<String>,
    pub max_retries: Option<i64>,
    pub executor: Option<String>,
    pub timeout_ms: Option<u64>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCapabilityPolicyApiRequest {
    pub description: String,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub requires_approval: bool,
    pub risk_level: String,
    pub expected_current_sha256: Option<String>,
    pub confirm_tool_policy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolAllowlistPolicyApiRequest {
    pub tool_names: Vec<String>,
    pub expected_current_sha256: Option<String>,
    pub confirm_tool_policy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolHookPolicyApiRequest {
    pub hook_type: String,
    pub tool_name: Option<String>,
    pub condition: Option<Value>,
    pub action: String,
    pub action_config: Option<Value>,
    pub enabled: bool,
    pub expected_current_sha256: Option<String>,
    pub confirm_tool_policy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SupervisedPatchWorkspaceCreateRequest {
    pub run_id: String,
    pub target_id: String,
    pub target_repo_path: String,
    pub source_revision: String,
    pub plan_id: Option<String>,
    pub source_tree_hash: Option<String>,
    pub workspace_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskVerificationCommandApi {
    pub command: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskExecutorPolicyApi {
    pub allowed_executors: Vec<String>,
    pub prefer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskBudgetApi {
    pub total_tokens: Option<u64>,
    pub total_calls: Option<u64>,
    pub total_elapsed_ms: Option<u64>,
    pub max_retries: Option<u64>,
    pub max_repairs: Option<u64>,
    pub max_concurrency: Option<u64>,
    pub stage_budgets: Option<Value>,
}

/// Canonical product golden-path intake contract (G1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskIntakeApiRequest {
    pub objective: String,
    pub target_id: String,
    pub target_repo_path: String,
    pub source_kind: Option<String>,
    pub source_revision: String,
    pub source_tree_hash: Option<String>,
    pub allowed_paths: Vec<String>,
    pub verification_commands: Vec<ProductTaskVerificationCommandApi>,
    pub output_intent: String,
    pub executor_policy: ProductTaskExecutorPolicyApi,
    pub budget: Option<ProductTaskBudgetApi>,
    pub risk_class: String,
    pub approval_required: Option<bool>,
    pub confirm_execution: Option<bool>,
    pub confirm_output: Option<bool>,
    pub idempotency_key: String,
    pub expected_version: Option<u64>,
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SupervisedPatchWorkspaceVerifyRequest {
    pub command: String,
    pub confirm_verification: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub attempt: Option<u64>,
    pub repair_executor: Option<String>,
    pub max_repair_attempts: Option<u64>,
    pub resume_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TargetRepoOutputRequest {
    pub run_id: String,
    pub mode: String,
    pub confirm_target_output: Option<bool>,
    pub branch_name: Option<String>,
    pub remote: Option<String>,
    pub commit_message: Option<String>,
    pub pr_title: Option<String>,
    pub create_pull_request: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SupervisedPatchArtifactRecordRequest {
    pub workspace_id: String,
    pub patch_hash: String,
    pub changed_files: Vec<String>,
    pub redaction_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BackupApiRequest {
    pub label: Option<String>,
    pub confirm_local_backup: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CreateApiKeyRequest {
    pub user_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UpdateKeyScopesRequest {
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CreateTeamMemberRequest {
    pub user_id: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ImportApiRequest {
    pub snapshot: serde_json::Value,
    pub confirm_import: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RestoreApiRequest {
    pub confirm_restore: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RestoreDryRunApiRequest {
    pub confirm_restore_dry_run: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PolicyProposalCreateRequest {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub task_class: Option<String>,
    pub task_domain: Option<String>,
    pub task_intent: Option<String>,
    pub tier: Option<String>,
    pub target_tier: Option<String>,
    pub payload: Option<Value>,
    pub evidence: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PolicyProposalActionRequest {
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub confirm_policy_override: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoAdjustmentApplyRequest {
    pub actor: Option<String>,
    pub candidate_id: Option<String>,
    pub confirm_auto_adjustment: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoAdjustmentRollbackRequest {
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub confirm_auto_adjustment_rollback: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptivePolicyPromotionApiRequest {
    pub actor: Option<String>,
    pub promotion: ContextualPolicyPromotion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptivePolicyRollbackApiRequest {
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub confirm_adaptive_policy_rollback: Option<bool>,
}

fn path_parameter(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": {"type": "string"}
    })
}

fn json_request_body(required: &[&str], properties: Value) -> Value {
    json!({
        "required": true,
        "content": {
            "application/json": {
                "schema": {
                    "type": "object",
                    "required": required,
                    "properties": properties
                }
            }
        }
    })
}

pub fn openapi_document() -> serde_json::Value {
    let mut doc = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Agent Control Plane Local API",
            "version": "0.1.0",
            "description": "Deterministic local API. Real providers, sandbox execution, target writes, and runtime workers are disabled by default."
        },
        "paths": {
            "/api/v1/health": {
                "get": {
                    "summary": "Health check",
                    "responses": {
                        "200": {"description": "API is healthy"}
                    }
                }
            },
            "/api/v1/ready": {
                "get": {
                    "summary": "Readiness check",
                    "responses": {
                        "200": {"description": "API is ready"}
                    }
                }
            },
            "/api/v1/openapi.json": {
                "get": {
                    "summary": "OpenAPI document",
                    "responses": {
                        "200": {"description": "OpenAPI JSON document"}
                    }
                }
            },
            "/api/v1/dispatch": {
                "post": {
                    "summary": "Create deterministic dispatch bundle",
                    "description": "Runs local rule-based dispatch only. The default executor is noop and does not call real providers.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["raw_request"],
                                    "properties": {
                                        "raw_request": {"type": "string"},
                                        "request_source": {"type": "string", "default": "api"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Dispatch bundle"},
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"},
                        "429": {"description": "Rate limited"}
                    }
                }
            }
            ,
            "/api/v1/adaptive-fusion/completions": {
                "post": {
                    "summary": "Run a guarded adaptive completion",
                    "description": "Requires configured auth plus provider and adaptive execution gates. Routing metadata is hidden unless explicitly requested.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["prompt"],
                                    "properties": {
                                        "prompt": {"type": "string"},
                                        "task_class": {"type": "string"},
                                        "objective": {"type": "string", "enum": ["efficient", "quality"]},
                                        "risk_level": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                                        "metadata": {"type": "object"},
                                        "include_routing_metadata": {"type": "boolean", "default": false}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Compact adaptive completion"},
                        "400": {"description": "Gate, validation, budget, kill, or execution block"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"},
                        "429": {"description": "Rate limited"}
                    }
                }
            },
            "/api/v1/dispatches": {
                "get": {
                    "summary": "List persisted local dispatch history",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}, "description": "Case-insensitive match across dispatch id, request text, source, status, tier, and risk."}
                    ],
                    "responses": {"200": {"description": "Dispatch history"}}
                }
            },
            "/api/v1/dispatches/{dispatch_id}": {
                "get": {
                    "summary": "Get a single dispatch by ID",
                    "parameters": [path_parameter("dispatch_id")],
                    "responses": {
                        "200": {"description": "Dispatch detail"},
                        "404": {"description": "Dispatch not found"}
                    }
                }
            },
            "/api/v1/dispatch-metrics": {
                "get": {
                    "summary": "Read derived dispatch metrics",
                    "description": "Requires dispatch:read scope. Derived from persisted dispatch history; does not affect routing or execution.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Dispatch metrics"}}
                }
            },
            "/api/v1/feedback/traces": {
                "get": {
                    "summary": "Read replayable feedback traces",
                    "description": "Requires dispatch:read scope. Derived from persisted dispatch bundles; read-only analysis.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "task_class", "in": "query", "schema": {"type": "string"}},
                        {"name": "tier", "in": "query", "schema": {"type": "string"}},
                        {"name": "status", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {"200": {"description": "Feedback traces"}}
                }
            },
            "/api/v1/feedback/cost-of-pass": {
                "get": {
                    "summary": "Read cost-of-pass aggregates",
                    "description": "Requires cost:read scope. Derived from persisted dispatch bundles; read-only analysis.",
                    "parameters": [
                        {"name": "task_class", "in": "query", "schema": {"type": "string"}},
                        {"name": "tier", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {"200": {"description": "Cost-of-pass aggregates"}}
                }
            },
            "/api/v1/simulation/report": {
                "get": {
                    "summary": "Read shadow simulation report",
                    "description": "Requires dispatch:read scope. Shadow routes are diagnostic only and cannot override active routing.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Shadow simulation report"}}
                }
            },
            "/api/v1/proposals": {
                "get": {
                    "summary": "List controlled-loop policy proposals",
                    "description": "Requires dispatch:read scope. Proposals are inactive until explicit human approval.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "status", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {"200": {"description": "Controlled-loop policy proposals"}}
                },
                "post": {
                    "summary": "Create controlled-loop policy proposal",
                    "description": "Creates a pending safe tier-map override proposal only; activation requires team:admin and confirm_policy_override.",
                    "requestBody": json_request_body(&[], json!({
                        "title": {"type": "string"},
                        "summary": {"type": "string"},
                        "task_domain": {"type": "string"},
                        "task_intent": {"type": "string"},
                        "target_tier": {"type": "string"},
                        "payload": {"type": "object"},
                        "evidence": {"type": "object"}
                    })),
                    "responses": {"200": {"description": "Created pending proposal"}}
                }
            },
            "/api/v1/proposals/{proposal_id}": {
                "get": {
                    "summary": "Get controlled-loop policy proposal",
                    "parameters": [path_parameter("proposal_id")],
                    "responses": {"200": {"description": "Proposal detail"}, "404": {"description": "Proposal not found"}}
                }
            },
            "/api/v1/proposals/{proposal_id}/approve": {
                "post": {
                    "summary": "Approve controlled-loop policy proposal",
                    "description": "Requires configured auth, team:admin scope, and confirm_policy_override=true.",
                    "parameters": [path_parameter("proposal_id")],
                    "requestBody": json_request_body(&["confirm_policy_override"], json!({
                        "actor": {"type": "string"},
                        "reason": {"type": "string"},
                        "confirm_policy_override": {"type": "boolean"}
                    })),
                    "responses": {"200": {"description": "Proposal activated"}, "400": {"description": "Missing confirmation or invalid proposal"}}
                }
            },
            "/api/v1/proposals/{proposal_id}/reject": {
                "post": {
                    "summary": "Reject controlled-loop policy proposal",
                    "description": "Requires configured auth and team:admin scope.",
                    "parameters": [path_parameter("proposal_id")],
                    "requestBody": json_request_body(&[], json!({
                        "actor": {"type": "string"},
                        "reason": {"type": "string"}
                    })),
                    "responses": {"200": {"description": "Proposal rejected"}}
                }
            },
            "/api/v1/proposals/{proposal_id}/deactivate": {
                "post": {
                    "summary": "Deactivate controlled-loop policy proposal",
                    "description": "Requires configured auth, team:admin scope, and confirm_policy_override=true.",
                    "parameters": [path_parameter("proposal_id")],
                    "requestBody": json_request_body(&["confirm_policy_override"], json!({
                        "actor": {"type": "string"},
                        "reason": {"type": "string"},
                        "confirm_policy_override": {"type": "boolean"}
                    })),
                    "responses": {"200": {"description": "Proposal deactivated"}}
                }
            },
            "/api/v1/proposals/{proposal_id}/rollback": {
                "post": {
                    "summary": "Rollback controlled-loop policy proposal",
                    "description": "Requires configured auth, team:admin scope, and confirm_policy_override=true.",
                    "parameters": [path_parameter("proposal_id")],
                    "requestBody": json_request_body(&["confirm_policy_override"], json!({
                        "actor": {"type": "string"},
                        "reason": {"type": "string"},
                        "confirm_policy_override": {"type": "boolean"}
                    })),
                    "responses": {"200": {"description": "Proposal rolled back"}}
                }
            },
            "/api/v1/auto-adjustments": {
                "get": {
                    "summary": "Read Phase 5 auto-adjustment report",
                    "description": "Requires dispatch:read scope. Default mode is disabled; dry-run is read-only; active mode requires both ACP_ENABLE_AUTO_ADJUSTMENT=1 and ACP_AUTO_ADJUSTMENT_ACTIVE=1.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Auto-adjustment report"}}
                }
            },
            "/api/v1/auto-adjustments/apply": {
                "post": {
                    "summary": "Apply one generated auto-adjustment",
                    "description": "Requires configured auth, team:admin scope, confirm_auto_adjustment=true, ACP_ENABLE_AUTO_ADJUSTMENT=1, ACP_AUTO_ADJUSTMENT_ACTIVE=1, and dry-run unset. Applies one safe tier-map override after persisting a rollback snapshot.",
                    "requestBody": json_request_body(&["confirm_auto_adjustment"], json!({
                        "actor": {"type": "string"},
                        "candidate_id": {"type": "string"},
                        "confirm_auto_adjustment": {"type": "boolean"}
                    })),
                    "responses": {"200": {"description": "Auto-adjustment apply result"}}
                }
            },
            "/api/v1/auto-adjustments/{adjustment_id}/rollback": {
                "post": {
                    "summary": "Rollback one auto-adjustment",
                    "description": "Requires configured auth, team:admin scope, confirm_auto_adjustment_rollback=true, and a valid snapshot safety hash.",
                    "parameters": [path_parameter("adjustment_id")],
                    "requestBody": json_request_body(&["confirm_auto_adjustment_rollback"], json!({
                        "actor": {"type": "string"},
                        "reason": {"type": "string"},
                        "confirm_auto_adjustment_rollback": {"type": "boolean"}
                    })),
                    "responses": {"200": {"description": "Auto-adjustment rollback result"}}
                }
            },
            "/api/v1/plans": {
                "get": {
                    "summary": "List persisted read-only workflow plans",
                    "description": "Requires dispatch:read scope. Plans are app-owned metadata only and include recommendation-only advisory status; they do not execute workers or write target repositories.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}, "description": "Case-insensitive match across plan id, request text, source, status, workflow id, and dispatch id."}
                    ],
                    "responses": {"200": {"description": "Read-only workflow plan list"}}
                },
                "post": {
                    "summary": "Create a read-only workflow plan",
                    "description": "Generates a canonical WorkflowGraph plan. Optional adaptive_execution or agent_steps creates an explicit bounded executable graph and requires dispatch:execute plus its matching confirmation. Each agent_steps entry is one leased action, and entries are dependency-ordered. When a provider is configured, every agent step must name that provider's exact default model; fixture-only states may omit it. Creating a plan does not execute a provider, worker, target write, deploy, merge, or approval action.",
                    "requestBody": json_request_body(&["raw_request"], json!({
                        "raw_request": {"type": "string"},
                        "request_source": {"type": "string", "default": "api"},
                        "adaptive_execution": {"type": "object", "description": "Optional explicit AdaptiveNodeExecutionConfig for one adaptive_provider node."},
                        "confirm_adaptive_execution_plan": {"type": "boolean"},
                        "agent_steps": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 16,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["agent_id", "role", "capability_profile", "profile_id"],
                                "properties": {
                                    "agent_id": {"type": "string", "maxLength": 256},
                                    "role": {"type": "string", "maxLength": 256},
                                    "capability_profile": {"type": "array", "minItems": 1, "maxItems": 16, "items": {"type": "string", "maxLength": 256}},
                                    "profile_id": {"type": "string", "maxLength": 256},
                                    "model": {"type": "string", "maxLength": 256, "description": "Required for provider-backed agent runtime plans and must exactly match the configured provider default model; optional only for deterministic fixture execution."}
                                }
                            }
                        },
                        "confirm_agent_runtime_plan": {"type": "boolean"}
                    })),
                    "responses": {
                        "200": {"description": "Read-only workflow plan"},
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/plans/{plan_id}": {
                "get": {
                    "summary": "Get a read-only workflow plan by ID",
                    "description": "Requires dispatch:read scope. Returns app-owned planning metadata plus recommendation-only advisory status.",
                    "parameters": [path_parameter("plan_id")],
                    "responses": {
                        "200": {"description": "Read-only workflow plan"},
                        "404": {"description": "Plan not found"}
                    }
                }
            },
            "/api/v1/workflow-runs": {
                "get": {
                    "summary": "List inert durable workflow run metadata",
                    "description": "Requires dispatch:read scope. Returns app-owned run metadata only; no workers, execution, provider calls, target writes, approval authority, deploy, or merge controls.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}}
                    ],
                    "responses": {"200": {"description": "Workflow run metadata list"}}
                },
                "post": {
                    "summary": "Create workflow run metadata from a plan",
                    "description": "Persists run/node/edge/event metadata. Plans containing agent_step or adaptive provider execution require dispatch:execute and confirm_execution=true because scheduler admission begins at run creation. No target-repository write authority is granted.",
                    "requestBody": json_request_body(&["plan_id"], json!({
                        "plan_id": {"type": "string"},
                        "confirm_execution": {"type": "boolean"}
                    })),
                    "responses": {
                        "200": {"description": "Workflow run metadata"},
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"},
                        "404": {"description": "Plan not found"}
                    }
                }
            },
            "/api/v1/workflow-runs/{run_id}": {
                "get": {
                    "summary": "Get inert workflow run metadata by ID",
                    "parameters": [path_parameter("run_id")],
                    "responses": {
                        "200": {"description": "Workflow run metadata"},
                        "404": {"description": "Workflow run not found"}
                    }
                }
            },
            "/api/v1/workflow-runs/{run_id}/nodes/{node_id}/external-runtime-checkpoint": {
                "get": {
                    "summary": "Inspect bounded managed external-runtime checkpoint metadata",
                    "description": "Returns scope-bound hashes, counters, IDs, versions, and summary-only checkpoint state. Raw prompts, outputs, transcripts, repository content, credentials, and private paths are excluded.",
                    "parameters": [path_parameter("run_id"), path_parameter("node_id"), {"name":"thread_id","in":"query","required":true,"schema":{"type":"string"}}],
                    "responses": {
                        "200": {"description": "Read-only external-runtime checkpoint metadata"},
                        "400": {"description": "thread_id is required"},
                        "404": {"description": "Authorized external-runtime scope not found"}
                    }
                }
            },
            "/api/v1/workflow-runs/{run_id}/events": {
                "get": {
                    "summary": "List workflow run metadata events",
                    "parameters": [path_parameter("run_id"), {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}],
                    "responses": {"200": {"description": "Workflow run metadata events"}}
                },
                "post": {
                    "summary": "Append workflow run metadata event",
                    "description": "Appends an event record only; does not trigger execution.",
                    "requestBody": json_request_body(&["event_type"], json!({
                        "node_id": {"type": "string"},
                        "event_type": {"type": "string"},
                        "details": {"type": "object"}
                    })),
                    "responses": {"200": {"description": "Workflow run metadata event"}}
                }
            },
            "/api/v1/workflow-runs/{run_id}/approvals": {
                "get": {
                    "summary": "List workflow run approval metadata",
                    "parameters": [path_parameter("run_id"), {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}],
                    "responses": {"200": {"description": "Workflow run approval metadata"}}
                },
                "post": {
                    "summary": "Record workflow run approval metadata",
                    "description": "Records approval metadata only; does not grant execution authority.",
                    "requestBody": json_request_body(&["node_id", "decision"], json!({
                        "node_id": {"type": "string"},
                        "decision": {"type": "string", "enum": ["requested", "approved", "rejected"]},
                        "reason": {"type": "string"}
                    })),
                    "responses": {"200": {"description": "Workflow run approval metadata"}}
                }
            },
            "/api/v1/workflow-runs/{run_id}/resume": {
                "post": {
                    "summary": "Record workflow run resume metadata",
                    "description": "Records resume intent and status metadata only; no execution resume authority.",
                    "parameters": [path_parameter("run_id")],
                    "requestBody": json_request_body(&[], json!({"reason": {"type": "string"}})),
                    "responses": {"200": {"description": "Workflow run metadata"}}
                }
            },
            "/api/v1/workflow-runs/{run_id}/cancel": {
                "post": {
                    "summary": "Record workflow run cancel metadata",
                    "description": "Records cancel intent and status metadata only; no worker or process cancellation authority.",
                    "parameters": [path_parameter("run_id")],
                    "requestBody": json_request_body(&[], json!({"reason": {"type": "string"}})),
                    "responses": {"200": {"description": "Workflow run metadata"}}
                }
            },
            "/api/v1/workflow-runs/{run_id}/tick": {
                "post": {
                    "summary": "Advance workflow run by one tick",
                    "description": "Finds a ready node, leases it through the existing workflow owner, executes through the explicitly selected bounded executor, and records the result. agent_step, Command, CLI, and provider paths retain their separate execution scopes and safety gates. Returns 409 if the run is already terminal.",
                    "parameters": [path_parameter("run_id")],
                    "requestBody": json_request_body(&[], json!({"actor": {"type": "string"}})),
                    "responses": {
                        "200": {"description": "Tick result with node execution details"},
                        "404": {"description": "Workflow run not found"},
                        "409": {"description": "Run is in terminal state"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces": {
                "get": {
                    "summary": "List supervised patch workspace metadata",
                    "description": "Requires dispatch:read scope. Returns app-owned Slice A metadata only; it does not create workspace directories, generate patches, execute workers, call providers, or write target repositories.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Supervised patch workspace metadata list"}}
                },
                "post": {
                    "summary": "Create a supervised patch workspace",
                    "description": "Creates an app-owned copy workspace with dispatch:read, or a controlled git_worktree with dispatch:execute plus ACP_ENABLE_TARGET_REPO_OUTPUT=1.",
                    "requestBody": json_request_body(&["run_id", "target_id", "target_repo_path", "source_revision"], json!({
                        "run_id": {"type": "string"},
                        "target_id": {"type": "string"},
                        "target_repo_path": {"type": "string"},
                        "source_revision": {"type": "string"},
                        "plan_id": {"type": "string"},
                        "source_tree_hash": {"type": "string"},
                        "workspace_mode": {"type": "string", "enum": ["copy", "git_worktree"], "default": "copy"}
                    })),
                    "responses": {
                        "200": {"description": "Created workspace metadata"},
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces/{workspace_id}": {
                "get": {
                    "summary": "Get supervised patch workspace metadata by ID",
                    "description": "Requires dispatch:read scope. Returns app-owned metadata only and grants no execution authority.",
                    "parameters": [path_parameter("workspace_id")],
                    "responses": {
                        "200": {"description": "Supervised patch workspace metadata"},
                        "404": {"description": "Supervised patch workspace not found"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces/{workspace_id}/cleanup": {
                "post": {
                    "summary": "Clean up a supervised patch workspace",
                    "description": "Removes the workspace directory and transitions status to cleaned. Copy cleanup requires dispatch:read; controlled git worktree cleanup requires dispatch:execute.",
                    "parameters": [path_parameter("workspace_id")],
                    "responses": {
                        "200": {"description": "Workspace cleaned up"},
                        "404": {"description": "Workspace not found"},
                        "409": {"description": "Invalid status transition"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces/{workspace_id}/quarantine": {
                "post": {
                    "summary": "Quarantine a supervised patch workspace",
                    "description": "Transitions workspace status to quarantined. Requires dispatch:read scope.",
                    "parameters": [path_parameter("workspace_id")],
                    "responses": {
                        "200": {"description": "Workspace quarantined"},
                        "404": {"description": "Workspace not found"},
                        "409": {"description": "Invalid status transition"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces/{workspace_id}/verify": {
                "post": {
                    "summary": "Run an allowlisted verification command",
                    "description": "Requires dispatch:execute and confirm_verification=true. Runs a fixed verification-tool allowlist inside the app-owned workspace with timeout, capped output, redaction, and persisted evidence.",
                    "parameters": [path_parameter("workspace_id")],
                    "requestBody": json_request_body(&["command", "confirm_verification"], json!({
                        "command": {"type": "string"},
                        "confirm_verification": {"type": "boolean"},
                        "timeout_ms": {"type": "integer", "minimum": 1000, "maximum": 600000},
                        "attempt": {"type": "integer", "minimum": 1, "maximum": 3},
                        "repair_executor": {"type": "string", "enum": ["codex_cli", "claude_code_cli"]},
                        "max_repair_attempts": {"type": "integer", "minimum": 1, "maximum": 2},
                        "resume_run_id": {"type": "string", "description": "Exact managed workflow run returned by an approval_required response"}
                    })),
                    "responses": {
                        "200": {"description": "Verification evidence recorded"},
                        "400": {"description": "Confirmation or command missing"},
                        "404": {"description": "Workspace not found"}
                    }
                }
            },
            "/api/v1/supervised-patch/workspaces/{workspace_id}/capture": {
                "post": {
                    "summary": "Capture patch and evidence from a supervised workspace",
                    "description": "Computes changed files, content-bound patch hash, review diff, workflow verification evidence, and secret scan state. Copy capture requires dispatch:read; controlled git worktree capture requires dispatch:execute.",
                    "parameters": [path_parameter("workspace_id")],
                    "responses": {
                        "200": {"description": "Captured supervised patch artifact"},
                        "404": {"description": "Workspace not found"},
                        "409": {"description": "Workspace has no capturable changes"}
                    }
                }
            },
            "/api/v1/supervised-patch/artifacts": {
                "get": {
                    "summary": "List supervised patch artifact metadata",
                    "description": "Requires dispatch:read scope. Returns app-owned artifact metadata only; it does not expose patch files, run redaction, approve export, apply patches, or mutate target repositories.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Supervised patch artifact metadata list"}}
                },
                "post": {
                    "summary": "Record a supervised patch artifact",
                    "description": "Records patch artifact metadata linked to a workspace. Requires dispatch:read scope.",
                    "requestBody": json_request_body(&["workspace_id", "patch_hash", "changed_files"], json!({
                        "workspace_id": {"type": "string"},
                        "patch_hash": {"type": "string"},
                        "changed_files": {"type": "array", "items": {"type": "string"}},
                        "redaction_status": {"type": "string", "enum": ["pending", "redacted", "failed"]}
                    })),
                    "responses": {
                        "200": {"description": "Recorded artifact metadata"},
                        "400": {"description": "Invalid request"},
                        "404": {"description": "Workspace not found"}
                    }
                }
            },
            "/api/v1/supervised-patch/artifacts/{artifact_id}": {
                "get": {
                    "summary": "Get supervised patch artifact metadata by ID",
                    "description": "Requires dispatch:read scope. Returns app-owned artifact metadata only and grants no patch apply/export authority.",
                    "parameters": [path_parameter("artifact_id")],
                    "responses": {
                        "200": {"description": "Supervised patch artifact metadata"},
                        "404": {"description": "Supervised patch artifact not found"}
                    }
                }
            },
            "/api/v1/supervised-patch/artifacts/{artifact_id}/export": {
                "post": {
                    "summary": "Export approval-bound artifact metadata",
                    "description": "Legacy app-owned metadata export. Requires valid approval binding and artifact integrity; does not push a target branch.",
                    "parameters": [path_parameter("artifact_id")],
                    "requestBody": json_request_body(&["run_id"], json!({
                        "run_id": {"type": "string"}
                    })),
                    "responses": {
                        "200": {"description": "Approval-bound artifact metadata export"},
                        "403": {"description": "Approval binding missing"},
                        "409": {"description": "Artifact integrity failed"}
                    }
                }
            },
            "/api/v1/supervised-patch/artifacts/{artifact_id}/output": {
                "post": {
                    "summary": "Export a real patch or push an approved PR branch",
                    "description": "Requires dispatch:execute scope, confirm_target_output=true, ACP_ENABLE_TARGET_REPO_OUTPUT=1, a controlled git_worktree, same-run approval binding, completed workflow verification evidence, artifact integrity, and passed secret scan. Branch pushes are restricted to acp/* and never write main. ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1 disables output immediately.",
                    "parameters": [path_parameter("artifact_id")],
                    "requestBody": json_request_body(&["run_id", "mode", "confirm_target_output"], json!({
                        "run_id": {"type": "string"},
                        "mode": {"type": "string", "enum": ["export_patch", "push_branch"]},
                        "confirm_target_output": {"type": "boolean"},
                        "branch_name": {"type": "string", "description": "Optional acp/* branch name"},
                        "remote": {"type": "string", "default": "origin"},
                        "commit_message": {"type": "string"},
                        "pr_title": {"type": "string"},
                        "create_pull_request": {"type": "boolean", "default": false}
                    })),
                    "responses": {
                        "200": {"description": "Real target output result"},
                        "400": {"description": "Confirmation or request validation failed"},
                        "403": {"description": "Scope or approval binding missing"},
                        "409": {"description": "Artifact/workspace changed or invalid"},
                        "503": {"description": "Target repo output gate disabled or kill switch active"}
                    }
                }
            },
            "/api/v1/dashboard": {
                "get": {
                    "summary": "Read local dashboard state from SQLite-backed runtime state",
                    "description": "Requires health:read. Tenant-filtered provider-embedding receipt evidence is included only for team:admin within the authenticated tenant; other callers receive an empty receipt view.",
                    "responses": {"200": {"description": "Dashboard state"}}
                }
            },
            "/api/v1/metrics": {
                "get": {
                    "summary": "Read local operational metrics",
                    "description": "Requires health:read scope. Reports dispatch, audit, key, backup, cost, token, provider, auth, and local boundary summary.",
                    "responses": {"200": {"description": "Operational metrics"}}
                }
            },
            "/api/v1/config": {
                "get": {
                    "summary": "Read local configuration",
                    "responses": {"200": {"description": "Local config"}}
                }
            },
            "/api/v1/team": {
                "get": {
                    "summary": "Read local team and redacted API key metadata",
                    "responses": {"200": {"description": "Team state"}}
                },
                "post": {
                    "summary": "Create or update a team member",
                    "description": "Requires team:admin scope.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["user_id", "display_name", "role"],
                                    "properties": {
                                        "user_id": {"type": "string"},
                                        "display_name": {"type": "string"},
                                        "role": {"type": "string"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Member created"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/team/{user_id}": {
                "put": {
                    "summary": "Update a team member's role",
                    "description": "Requires team:admin scope.",
                    "parameters": [path_parameter("user_id")],
                    "requestBody": json_request_body(&["role"], json!({
                        "role": {"type": "string"}
                    })),
                    "responses": {
                        "200": {"description": "Member updated"},
                        "404": {"description": "Member not found"}
                    }
                },
                "delete": {
                    "summary": "Remove a team member",
                    "description": "Requires team:admin scope.",
                    "parameters": [path_parameter("user_id")],
                    "responses": {
                        "200": {"description": "Member removed"},
                        "404": {"description": "Member not found"}
                    }
                }
            },
            "/api/v1/costs": {
                "get": {
                    "summary": "Read local cost summary from persisted dispatches",
                    "responses": {"200": {"description": "Cost summary"}}
                }
            },
            "/api/v1/costs/dispatches": {
                "get": {
                    "summary": "Read per-dispatch cost details",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 0, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Per-dispatch cost details"}}
                }
            },
            "/api/v1/export": {
                "get": {
                    "summary": "Export local app-owned state",
                    "responses": {"200": {"description": "Local export"}}
                }
            },
            "/api/v1/audit": {
                "get": {
                    "summary": "Read local audit log",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}, "description": "Case-insensitive match across audit actor, action, resource, and details."},
                        {"name": "redact", "in": "query", "schema": {"type": "boolean", "default": false}, "description": "When true, sensitive detail keys are redacted in the response."}
                    ],
                    "responses": {"200": {"description": "Audit log"}}
                }
            },
            "/api/v1/backups": {
                "get": {
                    "summary": "List local SQLite backups",
                    "description": "Requires backup:admin scope.",
                    "responses": {
                        "200": {"description": "Backup list"},
                        "403": {"description": "Forbidden"}
                    }
                },
                "post": {
                    "summary": "Create a local SQLite backup",
                    "description": "Requires backup:admin scope and confirm_local_backup=true.",
                    "requestBody": json_request_body(&["confirm_local_backup"], json!({
                        "label": {"type": "string"},
                        "confirm_local_backup": {"type": "boolean", "const": true}
                    })),
                    "responses": {
                        "200": {"description": "Backup metadata"},
                        "400": {"description": "Missing explicit confirmation"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}": {
                "delete": {
                    "summary": "Delete a local backup",
                    "description": "Requires backup:admin scope.",
                    "parameters": [path_parameter("backup_id")],
                    "responses": {
                        "200": {"description": "Backup deleted"},
                        "404": {"description": "Backup not found"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}/verify": {
                "get": {
                    "summary": "Verify a local backup",
                    "description": "Requires backup:admin scope. Checks backup checksum, SQLite integrity, and table row counts without modifying the live store.",
                    "parameters": [path_parameter("backup_id")],
                    "responses": {
                        "200": {"description": "Backup verification result"},
                        "404": {"description": "Backup not found"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/keys": {
                "get": {
                    "summary": "List API key metadata",
                    "description": "Requires team:read scope. Returns metadata only — no raw keys.",
                    "responses": {
                        "200": {"description": "List of API key metadata"},
                        "403": {"description": "Forbidden"}
                    }
                },
                "post": {
                    "summary": "Create a new API key",
                    "description": "Requires team:admin scope. Returns the raw key once — it cannot be retrieved later.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["user_id", "role", "scopes"],
                                    "properties": {
                                        "user_id": {"type": "string"},
                                        "role": {"type": "string"},
                                        "scopes": {"type": "array", "items": {"type": "string"}},
                                        "expires_at": {"type": "number"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Created key with raw_key"},
                        "400": {"description": "Invalid scopes or tenant"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/revoke": {
                "post": {
                    "summary": "Revoke an API key",
                    "description": "Requires team:admin scope. The key will no longer authenticate.",
                    "parameters": [path_parameter("key_id")],
                    "responses": {
                        "200": {"description": "Key revoked"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/rotate": {
                "post": {
                    "summary": "Rotate an API key",
                    "description": "Requires team:admin scope. Creates a new key and revokes the old one.",
                    "parameters": [path_parameter("key_id")],
                    "responses": {
                        "200": {"description": "New key with raw_key"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}": {
                "delete": {
                    "summary": "Delete an API key",
                    "description": "Requires team:admin scope. Hard-deletes key metadata.",
                    "parameters": [path_parameter("key_id")],
                    "responses": {
                        "200": {"description": "Key deleted"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/scopes": {
                "post": {
                    "summary": "Update an API key's scopes",
                    "description": "Requires team:admin scope.",
                    "parameters": [path_parameter("key_id")],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["scopes"],
                                    "properties": {
                                        "scopes": {"type": "array", "items": {"type": "string"}}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Scopes updated"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/decisions": {
                "get": {
                    "summary": "List orchestration decision log entries",
                    "description": "Requires dispatch:read scope. Returns persisted orchestration decision records with optional filtering by run_id and text search.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}, "description": "Case-insensitive match across run_id, action, selected_executor, and confidence."},
                        {"name": "run_id", "in": "query", "schema": {"type": "string"}, "description": "Filter decisions by workflow run ID."}
                    ],
                    "responses": {"200": {"description": "Decision log entries"}}
                }
            },
            "/api/v1/decisions/stats": {
                "get": {
                    "summary": "Orchestration decision log statistics",
                    "description": "Requires dispatch:read scope. Returns aggregate stats: total decisions, breakdown by action, and average confidence score.",
                    "responses": {"200": {"description": "Decision log statistics"}}
                }
            },
            "/api/v1/decisions/{decision_id}": {
                "get": {
                    "summary": "Get a single orchestration decision by ID",
                    "description": "Requires dispatch:read scope. Returns a single decision record with full input signals.",
                    "parameters": [path_parameter("decision_id")],
                    "responses": {
                        "200": {"description": "Decision detail"},
                        "404": {"description": "Decision not found"}
                    }
                }
            },
            "/api/v1/provider/health": {
                "get": {
                    "summary": "Provider health check",
                    "description": "Reports provider status: noop if no provider configured, ok if enabled, error if disabled or unavailable.",
                    "responses": {
                        "200": {"description": "Provider health status"}
                    }
                }
            },
            "/api/v1/scheduler/status": {
                "get": {
                    "summary": "Workflow scheduler status",
                    "description": "Reports bounded supervised-worker health, per-worker heartbeat, queue state, tick count, error count, gates, and configuration. Returns enabled=false when ACP_ENABLE_SCHEDULER is not set.",
                    "responses": {
                        "200": {"description": "Scheduler status"}
                    }
                }
            },
            "/api/v1/scheduler/control": {
                "post": {
                    "summary": "Pause, resume, or kill supervised workers",
                    "description": "Requires dispatch:execute scope and confirm_control=true. Every attempt is audited. Kill stops new claims immediately while any in-flight timeout-bounded execution drains; restart requires process/operator action and both scheduler gates.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["action", "confirm_control"],
                                    "properties": {
                                        "action": {"type": "string", "enum": ["pause", "resume", "kill"]},
                                        "actor": {"type": "string"},
                                        "confirm_control": {"type": "boolean"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Updated scheduler state"},
                        "400": {"description": "Invalid action or missing confirmation"},
                        "403": {"description": "Missing dispatch:execute scope"},
                        "409": {"description": "Scheduler unavailable or action rejected"}
                    }
                }
            },
            "/api/v1/provider/audit": {
                "get": {
                    "summary": "Read persisted provider audit events",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}}
                    ],
                    "responses": {
                        "200": {"description": "Provider audit event list"}
                    }
                }
            },
            "/api/v1/storage/integrity": {
                "get": {
                    "summary": "SQLite integrity check and table row counts",
                    "responses": {
                        "200": {"description": "Integrity report with per-table status"}
                    }
                }
            },
            "/api/v1/import": {
                "post": {
                    "summary": "Import data from an export snapshot",
                    "description": "Requires config:admin scope and confirm_import=true. Imports config, team, audit, and dispatches idempotently.",
                    "requestBody": json_request_body(&["snapshot", "confirm_import"], json!({
                        "snapshot": {"type": "object"},
                        "confirm_import": {"type": "boolean", "const": true}
                    })),
                    "responses": {
                        "200": {"description": "Import result with counts and errors"},
                        "400": {"description": "Missing confirmation or invalid schema"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}/restore": {
                "post": {
                    "summary": "Restore a backup with integrity verification",
                    "description": "Requires backup:admin scope and confirm_restore=true. Verifies checksum and integrity, restores through SQLite's online backup API into the currently owned connection, then rechecks integrity and reports row counts. It never replaces an open database inode.",
                    "parameters": [path_parameter("backup_id")],
                    "requestBody": json_request_body(&["confirm_restore"], json!({
                        "confirm_restore": {"type": "boolean", "const": true}
                    })),
                    "responses": {
                        "200": {"description": "Restore result"},
                        "400": {"description": "Missing confirmation"},
                        "404": {"description": "Backup not found"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}/restore/dry-run": {
                "post": {
                    "summary": "Dry-run a backup restore",
                    "description": "Requires backup:admin scope and confirm_restore_dry_run=true. Verifies the backup and reports whether restore would overwrite the live app-owned SQLite DB without modifying it.",
                    "parameters": [path_parameter("backup_id")],
                    "requestBody": json_request_body(&["confirm_restore_dry_run"], json!({
                        "confirm_restore_dry_run": {"type": "boolean", "const": true}
                    })),
                    "responses": {
                        "200": {"description": "Restore dry-run verification result"},
                        "400": {"description": "Missing confirmation"},
                        "404": {"description": "Backup not found"}
                    }
                }
            }
        }
    });
    append_provider_endpoint_openapi_paths(&mut doc);
    append_adaptive_fusion_openapi_paths(&mut doc);
    append_operator_evidence_openapi_paths(&mut doc);
    append_operator_decision_openapi_paths(&mut doc);
    append_operator_decision_action_openapi_paths(&mut doc);
    append_scorecard_openapi_paths(&mut doc);
    append_memory_openapi_paths(&mut doc);
    append_tool_policy_openapi_paths(&mut doc);
    append_delegated_product_task_openapi_paths(&mut doc);
    doc
}

fn append_delegated_product_task_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/product/tasks/{task_id}/delegated/prepare".to_string(),
        json!({
            "post": {
                "summary": "Approve and prepare one delegated managed ProductTask attempt",
                "description": "Requires the managed reviewer capabilities managed_acceptance:risk_acknowledge, managed_acceptance:delegated_autonomy, managed_acceptance:delegated_manifest_approve, and managed_acceptance:spend_authorize, plus the Product Golden Path gate. The route middleware checks the manifest-approval capability first; the store rechecks the complete set. The canonical bootstrap key owns only identity delegation and is not a managed-operation principal. Persists the authenticated manifest/spend approver, exact externally approved proposal and expected hash, delegation, immutable final manifest approval, and one-use spend authority. It does not admit a lease, activate execution, confirm output, or call a provider.",
                "parameters": [path_parameter("task_id")],
                "requestBody": json_request_body(
                    &[
                        "delegation",
                        "proposal_manifest",
                        "approved_proposal_sha256",
                        "attempt_id"
                    ],
                    json!({
                        "delegation": {"type": "object"},
                        "proposal_manifest": {"type": "object"},
                        "approved_proposal_sha256": {
                            "type": "string",
                            "pattern": "^[0-9a-f]{64}$"
                        },
                        "attempt_id": {"type": "string"}
                    }),
                ),
                "responses": {
                    "200": {"description": "Persisted proposal/delegation approval and one-use spend authority; execution_activated is false"},
                    "400": {"description": "Malformed or out-of-policy authority"},
                    "403": {"description": "Missing any required managed reviewer capability or disabled Product Golden Path"},
                    "409": {"description": "Scheduler, replay, or authority conflict"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/product/tasks/{task_id}/delegated/reconcile-unadmitted".to_string(),
        json!({
            "post": {
                "summary": "Reconcile one pre-admission delegated preparation",
                "description": "Requires the canonical bootstrap key and managed_acceptance:identity_delegate. The store first checks the ProductTask tenant and atomically revokes only an active delegation with no spend, attempt lease, artifact confirmation, or terminal receipt. Ordinary tenants, managed identities, and any admitted or outcome-bearing delegation are rejected.",
                "parameters": [path_parameter("task_id")],
                "requestBody": json_request_body(
                    &["delegation_id"],
                    json!({"delegation_id": {"type": "string"}}),
                ),
                "responses": {
                    "200": {"description": "Unadmitted delegation reconciled and audited"},
                    "400": {"description": "Delegation is not in the pre-admission state"},
                    "403": {"description": "Canonical bootstrap authority is required"},
                    "404": {"description": "ProductTask or delegation was not found"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/product/tasks/{task_id}/delegated/activate".to_string(),
        json!({
            "post": {
                "summary": "Consume delegated spend and activate the exact ProductTask attempt",
                "description": "Requires dispatch:execute, managed_acceptance:risk_acknowledge, managed_acceptance:delegated_execute, and managed_acceptance:attempt_admit. Persists an authenticated activator principal distinct from the manifest/spend approver, rechecks the final manifest and one-use spend identity, atomically admits the current lease, and activates the existing scheduler graph. It cannot issue approval or confirmation authority. The lease token is never returned.",
                "parameters": [path_parameter("task_id")],
                "requestBody": json_request_body(
                    &[
                        "delegation_id",
                        "attempt_id",
                        "final_manifest",
                        "spend_authorization_id"
                    ],
                    json!({
                        "delegation_id": {"type": "string"},
                        "attempt_id": {"type": "string"},
                        "final_manifest": {"type": "object"},
                        "spend_authorization_id": {"type": "string"}
                    }),
                ),
                "responses": {
                    "200": {"description": "Current attempt lease admitted and exact ProductTask graph activated"},
                    "400": {"description": "Malformed or mismatched activation binding"},
                    "403": {"description": "Missing dispatch:execute or delegated execution authority scope"},
                    "409": {"description": "Spend, lease, scheduler, replay, or authority conflict"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/product/tasks/{task_id}/delegated/approve".to_string(),
        json!({
            "post": {
                "summary": "Independently approve a delegated ProductTask artifact",
                "description": "Requires managed_acceptance:risk_acknowledge and managed_acceptance:delegated_artifact_confirm. The authenticated confirmer must belong to the same tenant as both the ProductTask and delegation, and must be distinct from the manifest/spend approver and execution activator. The store rechecks the exact artifact, verifier and Pro review evidence, provider journal, cost, target SHA, final manifest, and current delegation before authorizing one Draft-PR-only output.",
                "parameters": [path_parameter("task_id")],
                "requestBody": json_request_body(
                    &[
                        "expected_task_version",
                        "delegation_id",
                        "final_manifest",
                        "current_target_main_sha"
                    ],
                    json!({
                        "expected_task_version": {"type": "integer", "minimum": 0},
                        "delegation_id": {"type": "string"},
                        "final_manifest": {"type": "object"},
                        "current_target_main_sha": {
                            "type": "string",
                            "pattern": "^[0-9a-fA-F]{40}$"
                        }
                    }),
                ),
                "responses": {
                    "200": {"description": "Artifact/output confirmation persisted"},
                    "400": {"description": "Malformed, stale, or out-of-policy artifact"},
                    "403": {"description": "Missing artifact-confirmation scope or authenticated tenant does not own the ProductTask and delegation"},
                    "409": {"description": "Replay or authority conflict"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/product/tasks/{task_id}/delegated/terminal".to_string(),
        json!({
            "post": {
                "summary": "Close one delegated ProductTask attempt",
                "description": "Requires dispatch:execute scope. Rechecks Draft PR output and cleanup evidence, expires spend and delegation authority, closes the attempt lease, and persists terminal evidence.",
                "parameters": [path_parameter("task_id")],
                "requestBody": json_request_body(
                    &["delegation_id", "attempt_id"],
                    json!({
                        "delegation_id": {"type": "string"},
                        "attempt_id": {"type": "string"}
                    }),
                ),
                "responses": {
                    "200": {"description": "Terminal delegated evidence persisted"},
                    "400": {"description": "Missing or conflicting terminal evidence"},
                    "403": {"description": "Missing dispatch:execute scope"},
                    "409": {"description": "Late, duplicate, or conflicting terminal write"}
                }
            }
        }),
    );
}

fn append_provider_endpoint_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/provider/endpoints".to_string(),
        json!({
            "get": {
                "summary": "Read adaptive provider endpoint configuration",
                "description": "Requires config:read scope. Returns safe endpoint metadata from local config or environment. Credential values are symbolic environment variable names only; raw secrets are never returned.",
                "responses": {"200": {"description": "Provider endpoint configuration"}}
            },
            "put": {
                "summary": "Save adaptive provider endpoint configuration",
                "description": "Requires config:admin scope and confirm_provider_endpoint_config=true. Persists endpoint metadata and credential environment names only; raw secrets are rejected.",
                "requestBody": json_request_body(&["endpoints", "confirm_provider_endpoint_config"], json!({
                    "endpoints": {"type": "array"},
                    "confirm_provider_endpoint_config": {"type": "boolean", "const": true}
                })),
                "responses": {
                    "200": {"description": "Saved provider endpoint configuration"},
                    "400": {"description": "Invalid endpoint config or missing confirmation"},
                    "403": {"description": "Missing config:admin scope"}
                }
            }
        }),
    );
}

fn append_adaptive_fusion_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/adaptive-fusion/policies".to_string(),
        json!({
            "get": {
                "summary": "List active adaptive fusion policies",
                "description": "Requires dispatch:read scope. Policies have no live execution authority by themselves and require explicit adaptive candidate plans.",
                "responses": {"200": {"description": "Adaptive fusion policy list"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/adaptive-fusion/policies/promote".to_string(),
        json!({
            "post": {
                "summary": "Deprecated caller-asserted policy promotion",
                "description": "Requires team:admin only to return a stable deprecation response. This route never mutates policy; callers must use promote-with-evidence.",
                "requestBody": json_request_body(&["promotion"], json!({
                    "actor": {"type": "string"},
                    "promotion": {"type": "object"}
                })),
                "responses": {"410": {"description": "Legacy promotion route is permanently deprecated"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/adaptive-fusion/policies/promote-with-evidence".to_string(),
        json!({
            "post": {
                "summary": "Promote a replay-bound policy through the complete evidence chain",
                "description": "Requires configured auth, team:admin, exact immutable replay binding, mutation-time trace rebinding, explicit confirmation, enabled canary/promotion gates, snapshot creation, and rollback target.",
                "requestBody": json_request_body(&["replay_artifact_id", "promotion", "canary", "rollout_scope", "rollback_target", "confirm_promotion"], json!({
                    "replay_artifact_id": {"type":"string"},
                    "promotion": {"type":"object"},
                    "canary": {"type":"object"},
                    "rollout_scope": {"type":"string"},
                    "rollback_target": {"type":"string"},
                    "confirm_promotion": {"type":"boolean", "const":true}
                })),
                "responses": {"200":{"description":"Evidence-chain promotion and rollback snapshot"},"400":{"description":"Incomplete, stale, changed, or policy-rejected evidence"},"403":{"description":"Missing team:admin"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/adaptive-fusion/policies/{adjustment_id}/rollback".to_string(),
        json!({
            "post": {
                "summary": "Rollback one adaptive fusion policy",
                "description": "Requires configured auth, team:admin scope, confirm_adaptive_policy_rollback=true, and a valid adaptive policy snapshot hash.",
                "parameters": [path_parameter("adjustment_id")],
                "requestBody": json_request_body(&["confirm_adaptive_policy_rollback"], json!({
                    "actor": {"type": "string"},
                    "reason": {"type": "string"},
                    "confirm_adaptive_policy_rollback": {"type": "boolean"}
                })),
                "responses": {"200": {"description": "Adaptive fusion policy rollback result"}}
            }
        }),
    );
}

fn append_operator_evidence_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/operator/evidence/{run_id}".to_string(),
        json!({
            "get": {
                "summary": "Read operator evidence read-model for a workflow run",
                "description": "Requires dispatch:read scope. Aggregated agent state, mailbox counts, proposal counts by type, blocked signals, and sanitized audit events. Metadata-only — no raw prompts, outputs, rationales, scratchpads, or secrets.",
                "parameters": [path_parameter("run_id")],
                "responses": {
                    "200": {"description": "Operator evidence read-model"},
                    "404": {"description": "Run not found (returns empty evidence)"}
                }
            }
        }),
    );
}

fn append_operator_decision_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/operator/decisions".to_string(),
        json!({
            "get": {
                "summary": "Read the bounded derived operator decision queue",
                "description": "Requires dispatch:read scope. Recomputes operator_decision_queue.v1 from existing evidence owners. Read-only and metadata-only: no approval, pause, resume, retry, rollback, provider, or target-repository authority is granted.",
                "parameters": [
                    {"name": "generated_at", "in": "query", "schema": {"type": "string", "format": "date-time"}},
                    {"name": "maximum_freshness_seconds", "in": "query", "schema": {"type": "integer", "default": 300, "minimum": 1, "maximum": 2592000}},
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}},
                    {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0, "maximum": 10000}}
                ],
                "responses": {"200": {"description": "Deterministic derived decision queue"}, "400": {"description": "Invalid queue query"}, "403": {"description": "Missing dispatch:read scope"}, "500": {"description": "An evidence owner could not be read"}}
            }
        }),
    );
}

fn append_operator_decision_action_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/operator/decisions/{decision_id}/actions".to_string(),
        json!({
            "post": {
                "summary": "Apply one hash-bound allowlisted operator decision through its existing owner",
                "description": "Requires explicit confirmation and the exact current derived queue page binding (hash, timestamp, freshness, limit, and offset). The adapter re-derives the decision and invokes only the existing approval, workflow resume/retry, or budget auto-pause owner. Unsupported actions fail closed. This route adds no general execution authority.",
                "parameters": [path_parameter("decision_id")],
                "responses": {"200": {"description": "Existing owner result"}, "400": {"description": "Confirmation or required budget policy missing"}, "403": {"description": "Missing owner permission"}, "404": {"description": "Decision absent from bound queue"}, "409": {"description": "Queue/source changed, decision not ready, or owner rejected action"}}
            }
        }),
    );
}

fn append_scorecard_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/scorecards".to_string(),
        json!({
            "get": {
                "summary": "List scorecard artifacts by run, dispatch, or scenario",
                "description": "Requires dispatch:read scope. Returns app-owned read-only native_scorecard_artifact.v1 or scorecard_artifact.v2 envelopes from the existing LocalProductStore table. Scenario queries include an explicit baseline/candidate comparison. No raw traces or target repository writes are exposed.",
                "parameters": [
                    {"name": "run_id", "in": "query", "schema": {"type": "string"}},
                    {"name": "dispatch_id", "in": "query", "schema": {"type": "string"}},
                    {"name": "scenario_id", "in": "query", "schema": {"type": "string"}},
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}}
                ],
                "responses": {
                    "200": {"description": "Scorecard artifact list and optional scenario comparison"},
                    "400": {"description": "Missing or ambiguous query scope"},
                    "422": {"description": "Scenario artifacts are not comparable"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/scorecards/{artifact_id}".to_string(),
        json!({
            "get": {
                "summary": "Get scorecard artifact by ID",
                "description": "Requires dispatch:read scope. Returns one app-owned read-only native_scorecard_artifact.v1 or scorecard_artifact.v2 envelope.",
                "parameters": [path_parameter("artifact_id")],
                "responses": {
                    "200": {"description": "Scorecard artifact"},
                    "404": {"description": "Scorecard artifact not found"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/regressions".to_string(),
        json!({
            "get": {
                "summary": "List bounded token-efficiency regression artifacts",
                "description": "Requires dispatch:read scope. Returns read-only metadata-bounded report or batch envelopes from the existing LocalProductStore boundary. No provider calls, raw payloads, or mutation authority.",
                "parameters": [
                    {"name": "scenario_id", "in": "query", "schema": {"type": "string"}},
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}}
                ],
                "responses": {"200": {"description": "Regression artifact list"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/regressions/{artifact_id}".to_string(),
        json!({
            "get": {
                "summary": "Get one bounded token-efficiency regression artifact",
                "description": "Requires dispatch:read scope. Returns one validated read-only regression report or batch envelope.",
                "parameters": [path_parameter("artifact_id")],
                "responses": {
                    "200": {"description": "Regression artifact"},
                    "404": {"description": "Regression artifact not found"}
                }
            }
        }),
    );
    paths.insert(
        "/api/v1/regressions/trends/{scenario_id}".to_string(),
        json!({
            "get": {
                "summary": "Get deterministic bounded regression history and trend",
                "description": "Requires dispatch:read scope. Derives up to 100 recent points with outcome, reason, evidence-link, and metric-direction transitions; report-only and mutation-free.",
                "parameters": [
                    path_parameter("scenario_id"),
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}}
                ],
                "responses": {"200": {"description": "Regression trend read model"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/budget-evidence".to_string(),
        json!({
            "get": {
                "summary": "List bounded budget forecast and anomaly evidence",
                "description": "Requires dispatch:read scope. Returns immutable, metadata-only validated budget evidence artifacts from LocalProductStore. No provider calls, pause action, policy mutation, or budget mutation authority is granted.",
                "parameters": [
                    {"name": "kind", "in": "query", "schema": {"type": "string", "enum": ["forecast", "anomaly"]}},
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}},
                    {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0, "maximum": 10000}}
                ],
                "responses": {"200": {"description": "Budget evidence artifact list"}, "400": {"description": "Invalid kind"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/budget-evidence/{artifact_id}".to_string(),
        json!({
            "get": {
                "summary": "Get one bounded budget evidence artifact",
                "description": "Requires dispatch:read scope. Returns one immutable validated forecast or anomaly envelope.",
                "parameters": [path_parameter("artifact_id")],
                "responses": {"200": {"description": "Budget evidence artifact"}, "404": {"description": "Budget evidence artifact not found"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/usage-observations".to_string(),
        json!({
            "get": {
                "summary": "List normalized usage observations for one run",
                "description": "Requires dispatch:read and exact tenant-bound run ownership. Returns source/hash identity, completeness, confidence, pricing identity, and metric provenance without provider content.",
                "parameters":[{"name":"run_id","in":"query","required":true,"schema":{"type":"string"}},{"name":"limit","in":"query","schema":{"type":"integer","default":64,"minimum":1,"maximum":64}}],
                "responses":{"200":{"description":"Metadata-only normalized observations"},"404":{"description":"Run not found in authenticated tenant"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/budget-evidence/recompute".to_string(),
        json!({
            "post": {
                "summary":"Recompute budget intelligence for an exact run",
                "description":"Requires dispatch:execute, tenant-bound run ownership, and explicit confirmation. The idempotent producer deduplicates normalized sources and immutable artifacts.",
                "requestBody":json_request_body(&["run_id","confirm_recompute"],json!({"run_id":{"type":"string"},"confirm_recompute":{"type":"boolean","const":true}})),
                "responses":{"200":{"description":"Producer result and normalized observation provenance"},"400":{"description":"Confirmation missing"},"404":{"description":"Run not found"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/offline-replays".to_string(),
        json!({
            "get": {
                "summary": "List bounded trace-backed offline replay evidence",
                "description": "Requires dispatch:read scope. Returns immutable, hash-bound replay reports derived from accepted owner-backed trace evidence. Current v2 reports are eligible for downstream validation; readable v1 historical reports are explicitly non-authorizing. Results are insufficient or invalid when evidence cannot establish comparability; no provider calls or production mutation are allowed.",
                "parameters": [
                    {"name": "status", "in": "query", "schema": {"type": "string", "enum": ["sufficient", "insufficient_evidence", "incompatible_cohort", "stale_evidence", "tampered_evidence", "uncalibrated_evidence", "out_of_distribution"]}},
                    {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "minimum": 1, "maximum": 100}},
                    {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0, "maximum": 10000}}
                ],
                "responses": {"200": {"description": "Offline replay evidence list"}, "400": {"description": "Invalid status"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/offline-replays/{artifact_id}".to_string(),
        json!({
            "get": {
                "summary": "Get one bounded trace-backed offline replay report",
                "description": "Requires dispatch:read scope. Returns one validated, immutable replay report with source evidence hashes and no mutation authority. Legacy v1 reports remain historical-only and cannot authorize shadow, canary, or promotion.",
                "parameters": [path_parameter("artifact_id")],
                "responses": {"200": {"description": "Offline replay evidence artifact"}, "404": {"description": "Offline replay artifact not found"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/offline-replays/generate".to_string(),
        json!({
            "post": {
                "summary":"Generate immutable replay evidence from trusted dispatch traces",
                "description":"Requires dispatch:execute and explicit confirmation. Provider calls remain disabled; ineligible traces produce explicit replay status and reason codes.",
                "requestBody":json_request_body(&["replay","confirm_generation"],json!({"replay":{"type":"object"},"confirm_generation":{"type":"boolean","const":true}})),
                "responses":{"200":{"description":"Idempotent replay producer result"},"400":{"description":"Invalid or unbound replay request"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/offline-replays/production-profile".to_string(),
        json!({
            "get": {
                "summary":"Inspect the bounded automatic replay producer profile",
                "description":"Requires dispatch:read. Returns configuration metadata only and grants no mutation authority.",
                "responses":{"200":{"description":"Current profile or explicit unconfigured state"}}
            },
            "put": {
                "summary":"Configure the bounded automatic replay producer profile",
                "description":"Requires configured authentication, team:admin, and explicit confirmation. The profile is app-owned; dispatch completion remains the only automatic trigger and never calls providers.",
                "requestBody":json_request_body(&["profile","confirm_profile"],json!({
                    "profile":{"type":"object","required":["enabled","bounded_dispatch_window","maximum_trace_age_seconds","current_policy","candidate_policies"]},
                    "confirm_profile":{"type":"boolean","const":true}
                })),
                "responses":{"200":{"description":"Persisted profile"},"400":{"description":"Invalid profile or confirmation missing"},"403":{"description":"Configured admin authentication required"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/budget-evidence/{artifact_id}/auto-pause".to_string(),
        json!({
            "post": {
                "summary": "Apply a policy-gated budget anomaly pause",
                "description": "Requires dispatch:execute scope and explicit confirmation. Default-off policy, validated high-confidence fresh critical evidence, persistent audit, and the existing workflow pause owner are required.",
                "parameters": [path_parameter("artifact_id")],
                "responses": {"200": {"description": "Idempotent pause decision"}, "400": {"description": "Confirmation missing"}, "403": {"description": "Missing permission"}, "409": {"description": "Policy or evidence rejected"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/budget-pauses/{run_id}/recovery".to_string(),
        json!({
            "post": {
                "summary": "Resume or override an audited budget pause",
                "description": "Requires dispatch:execute scope, explicit confirmation, recovery mode, and bounded operator reason. Cause and evidence hashes remain persisted.",
                "parameters": [path_parameter("run_id")],
                "responses": {"200": {"description": "Audited recovery decision"}, "400": {"description": "Confirmation missing"}, "403": {"description": "Missing permission"}, "409": {"description": "Recovery rejected"}}
            }
        }),
    );
}

fn append_memory_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/memories".to_string(),
        json!({"post":{"summary":"Create one immutable durable-memory version","description":"Requires dispatch:execute. Tenant/workspace scope is rebound to the required authoritative run_id; embeddings are separately gated and provider mode is default-off.","requestBody":json_request_body(&["scope","run_id","source_id","source_sha256","conflict_key","content","confidence"],json!({"scope":{"type":"object","required":["tenant_id","workspace_id"],"properties":{"tenant_id":{"type":"string"},"workspace_id":{"type":"string"},"agent_id":{"type":["string","null"]},"task_id":{"type":["string","null"]}}},"run_id":{"type":"string"},"source_id":{"type":"string"},"source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"conflict_key":{"type":"string"},"content":{},"confidence":{"type":"number","minimum":0,"maximum":1},"fresh_until":{"type":["string","null"],"format":"date-time"},"expires_at":{"type":["string","null"],"format":"date-time"},"supersedes_memory_id":{"type":["string","null"],"description":"Must be null on create; use the atomic supersede route."}})),"responses":{"201":{"description":"Created or exact idempotent memory"},"400":{"description":"Invalid, oversized, or conflicting binding"},"403":{"description":"Tenant, workspace, agent, or task scope mismatch"}}}}),
    );
    paths.insert(
        "/api/v1/memories/{memory_id}".to_string(),
        json!({"get":{"summary":"Inspect durable-memory version history","description":"Requires dispatch:read and an authoritative run_id whose tenant/workspace matches the stored memory scope.","parameters":[path_parameter("memory_id"),{"name":"run_id","in":"query","required":true,"schema":{"type":"string"}}],"responses":{"200":{"description":"Immutable version history"},"403":{"description":"Run scope mismatch"},"404":{"description":"Not found in authenticated tenant"}}}}),
    );
    for (suffix, summary) in [
        (
            "revise",
            "Revise durable memory with optimistic version binding",
        ),
        (
            "invalidate",
            "Invalidate durable memory with optimistic version binding",
        ),
        (
            "forget",
            "Tombstone durable memory with optimistic version binding",
        ),
    ] {
        paths.insert(
            format!("/api/v1/memories/{{memory_id}}/{suffix}"),
            json!({"post":{"summary":summary,"description":"Requires dispatch:execute, an exact run/scope rebinding, and exact expected_version. Mutation and audit commit atomically.","parameters":[path_parameter("memory_id")],"requestBody":if suffix == "revise" { json_request_body(&["run_id","scope","expected_version","source_id","source_sha256","content","confidence"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"]},"expected_version":{"type":"integer","minimum":1},"source_id":{"type":"string"},"source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"content":{},"confidence":{"type":"number","minimum":0,"maximum":1},"fresh_until":{"type":["string","null"],"format":"date-time"},"expires_at":{"type":["string","null"],"format":"date-time"}})) } else { json_request_body(&["run_id","scope","expected_version"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"]},"expected_version":{"type":"integer","minimum":1}})) },"responses":{"200":{"description":"New immutable memory version"},"403":{"description":"Run or memory scope mismatch"},"409":{"description":"Version conflict"}}}}),
        );
    }
    paths.insert(
        "/api/v1/memories/{memory_id}/reembed".to_string(),
        json!({"post":{"summary":"Re-embed one current durable-memory version under the pinned provider contract","description":"Requires dispatch:execute, exact run/scope/version binding, provider mode, all provider safety gates, and explicit confirmation. Creates a new immutable version; no historical version is rewritten.","parameters":[path_parameter("memory_id")],"requestBody":json_request_body(&["run_id","scope","expected_version","confirm_reembed"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"]},"expected_version":{"type":"integer","minimum":1},"confirm_reembed":{"type":"boolean","const":true}})),"responses":{"200":{"description":"New immutable provider-embedded memory version"},"400":{"description":"Confirmation, provider contract, or gate invalid"},"403":{"description":"Run or memory scope mismatch"},"409":{"description":"Version conflict"}}}}),
    );
    paths.insert(
        "/api/v1/memories/{memory_id}/embedding/reconcile".to_string(),
        json!({"post":{"summary":"Reconcile one provider-embedding failure receipt","description":"Requires dispatch:execute, exact run/scope/target/attempt binding, and explicit confirmation. Known failures may receive a bounded retry authorization. Unknown outcomes can only be acknowledged with a bounded source/hash audit reference and remain permanently blocked from another POST. Mutation and audit commit atomically; known-failure attempts are capped at four.","parameters":[path_parameter("memory_id")],"requestBody":json_request_body(&["run_id","scope","target_version","expected_attempt_count","action","confirm_resolution"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"]},"target_version":{"type":"integer","minimum":1},"expected_attempt_count":{"type":"integer","minimum":1,"maximum":4},"action":{"type":"string","enum":["retry_failed","acknowledge_unknown"]},"evidence_source_id":{"type":["string","null"],"maxLength":256},"evidence_sha256":{"type":["string","null"],"pattern":"^[0-9a-f]{64}$"},"confirm_resolution":{"type":"boolean","const":true}})),"responses":{"200":{"description":"Hash-bound reconciliation result"},"400":{"description":"Evidence, state, attempt, or confirmation invalid"},"403":{"description":"Run or operation scope mismatch"}}}}),
    );
    paths.insert(
        "/api/v1/memories/{memory_id}/supersede".to_string(),
        json!({"post":{"summary":"Resolve exactly one conflicting fact pair atomically","description":"Requires dispatch:execute, exact run/scope ownership for both identities, exact versions, and explicit confirmation.","parameters":[path_parameter("memory_id")],"requestBody":json_request_body(&["run_id","scope","winner_expected_version","loser_memory_id","loser_expected_version","confirm_supersede"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"]},"winner_expected_version":{"type":"integer","minimum":1},"loser_memory_id":{"type":"string"},"loser_expected_version":{"type":"integer","minimum":1},"confirm_supersede":{"type":"boolean","const":true}})),"responses":{"200":{"description":"Winner and superseded immutable versions"},"400":{"description":"Confirmation or conflict-pair validation failed"},"403":{"description":"Run or memory scope mismatch"},"409":{"description":"Version conflict"}}}}),
    );
    paths.insert(
        "/api/v1/memories/prune".to_string(),
        json!({"post":{"summary":"Prune a bounded exact scope of expired current memories","description":"Requires dispatch:execute, authoritative run/scope ownership, and explicit confirmation. At most 100 transitions are committed atomically.","requestBody":json_request_body(&["run_id","scope","confirm_prune"],json!({"run_id":{"type":"string"},"scope":{"type":"object","required":["tenant_id","workspace_id"],"properties":{"tenant_id":{"type":"string"},"workspace_id":{"type":"string"},"agent_id":{"type":["string","null"]},"task_id":{"type":["string","null"]}}},"confirm_prune":{"type":"boolean","const":true}})),"responses":{"200":{"description":"Bounded prune result"},"400":{"description":"Confirmation or scope invalid"},"403":{"description":"Run or scope mismatch"}}}}),
    );
    paths.insert(
        "/api/v1/memories/retrieve".to_string(),
        json!({"post":{"summary":"Retrieve bounded hash-bound durable-memory references","description":"Requires dispatch:read and exact tenant/workspace/run scope. Semantic vector mode is gated; lexical fallback is explicit and labeled.","requestBody":json_request_body(&["scope","run_id","node_id","query","top_k","max_tokens","max_bytes"],json!({"scope":{"type":"object","required":["tenant_id","workspace_id"]},"run_id":{"type":"string"},"node_id":{"type":"string"},"query":{"type":"string","maxLength":16384},"top_k":{"type":"integer","minimum":1,"maximum":20},"max_tokens":{"type":"integer","minimum":1,"maximum":32768},"max_bytes":{"type":"integer","minimum":1,"maximum":131072},"allow_lexical_fallback":{"type":"boolean","default":false}})),"responses":{"200":{"description":"Deterministic Top-K result with provenance, scores, exclusions, and budgets"},"403":{"description":"Tenant or workspace mismatch"}}}}),
    );
}

fn append_tool_policy_openapi_paths(doc: &mut Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    paths.insert(
        "/api/v1/tool-policy/capabilities/{tool_name}".to_string(),
        json!({
            "get": {
                "summary": "Inspect one hash-bound tool capability policy",
                "description": "Requires dispatch:read scope. Returns bounded policy metadata and its canonical SHA-256.",
                "parameters": [path_parameter("tool_name")],
                "responses": {"200": {"description": "Tool capability policy"}, "404": {"description": "Not found"}}
            },
            "put": {
                "summary": "Create or replace one tool capability policy",
                "description": "Requires dispatch:execute, explicit confirmation, and the current SHA-256 when replacing an existing resource. Mutation and audit commit atomically.",
                "parameters": [path_parameter("tool_name")],
                "requestBody": json_request_body(&["description", "requires_approval", "risk_level", "confirm_tool_policy"], json!({
                    "description": {"type": "string", "maxLength": 4096},
                    "input_schema": {"type": "object"},
                    "output_schema": {"type": "object"},
                    "requires_approval": {"type": "boolean"},
                    "risk_level": {"type": "string", "enum": ["low", "medium", "high"]},
                    "expected_current_sha256": {"type": "string", "minLength": 64, "maxLength": 64},
                    "confirm_tool_policy": {"type": "boolean", "const": true}
                })),
                "responses": {"200": {"description": "Hash-bound policy resource"}, "400": {"description": "Invalid or unconfirmed policy"}, "409": {"description": "Stale policy binding"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/tool-policy/profiles/{profile_id}/allowlist".to_string(),
        json!({
            "get": {
                "summary": "Inspect one configured tool allowlist",
                "description": "Requires dispatch:read scope. An absent resource means legacy unconfigured behavior; a present empty list is an authoritative deny-all policy.",
                "parameters": [path_parameter("profile_id")],
                "responses": {"200": {"description": "Tool allowlist policy"}, "404": {"description": "Profile is unconfigured"}}
            },
            "put": {
                "summary": "Create or replace one authoritative tool allowlist",
                "description": "Requires dispatch:execute, explicit confirmation, registered capabilities, and the current SHA-256 when replacing an existing resource.",
                "parameters": [path_parameter("profile_id")],
                "requestBody": json_request_body(&["tool_names", "confirm_tool_policy"], json!({
                    "tool_names": {"type": "array", "maxItems": 128, "uniqueItems": true, "items": {"type": "string", "maxLength": 256}},
                    "expected_current_sha256": {"type": "string", "minLength": 64, "maxLength": 64},
                    "confirm_tool_policy": {"type": "boolean", "const": true}
                })),
                "responses": {"200": {"description": "Hash-bound policy resource"}, "400": {"description": "Invalid or unconfirmed policy"}, "409": {"description": "Stale policy binding"}}
            }
        }),
    );
    paths.insert(
        "/api/v1/tool-policy/hooks/{hook_id}".to_string(),
        json!({
            "get": {
                "summary": "Inspect one hash-bound tool hook",
                "description": "Requires dispatch:read scope.",
                "parameters": [path_parameter("hook_id")],
                "responses": {"200": {"description": "Tool hook policy"}, "404": {"description": "Not found"}}
            },
            "put": {
                "summary": "Create or replace one bounded tool hook",
                "description": "Requires dispatch:execute, explicit confirmation, bounded non-sensitive metadata, and the current SHA-256 when replacing an existing resource. Post-execution approval requests are rejected.",
                "parameters": [path_parameter("hook_id")],
                "requestBody": json_request_body(&["hook_type", "action", "enabled", "confirm_tool_policy"], json!({
                    "hook_type": {"type": "string", "enum": ["pre_execution", "post_execution"]},
                    "tool_name": {"type": "string", "maxLength": 256},
                    "condition": {"type": "object"},
                    "action": {"type": "string", "enum": ["log", "block", "enrich", "request_approval"]},
                    "action_config": {"type": "object"},
                    "enabled": {"type": "boolean"},
                    "expected_current_sha256": {"type": "string", "minLength": 64, "maxLength": 64},
                    "confirm_tool_policy": {"type": "boolean", "const": true}
                })),
                "responses": {"200": {"description": "Hash-bound policy resource"}, "400": {"description": "Invalid or unconfirmed policy"}, "409": {"description": "Stale policy binding"}}
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_prefix, "/api/v1");
    }

    #[test]
    fn test_openapi_integrity_route_matches_router() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().expect("paths should be an object");
        assert!(
            paths.contains_key("/api/v1/adaptive-fusion/completions"),
            "OpenAPI document must include the guarded adaptive completion route"
        );
        assert!(
            paths.contains_key("/api/v1/storage/integrity"),
            "OpenAPI document must include /api/v1/storage/integrity to match the axum router registration"
        );
        for path in [
            "/api/v1/operator/decisions",
            "/api/v1/operator/decisions/{decision_id}/actions",
            "/api/v1/regressions",
            "/api/v1/regressions/{artifact_id}",
            "/api/v1/regressions/trends/{scenario_id}",
            "/api/v1/tool-policy/capabilities/{tool_name}",
            "/api/v1/tool-policy/profiles/{profile_id}/allowlist",
            "/api/v1/tool-policy/hooks/{hook_id}",
        ] {
            assert!(
                paths.contains_key(path),
                "OpenAPI document must include {path} to match the axum router registration"
            );
        }
        assert!(
            !paths.contains_key("/api/v1/integrity"),
            "OpenAPI document must NOT include /api/v1/integrity (the correct path is /api/v1/storage/integrity)"
        );
    }

    #[test]
    fn test_openapi_dynamic_routes_document_path_parameters() {
        let doc = openapi_document();

        assert_path_parameter(
            &doc,
            "/api/v1/dispatches/{dispatch_id}",
            "get",
            "dispatch_id",
        );
        assert_path_parameter(&doc, "/api/v1/plans/{plan_id}", "get", "plan_id");
        assert_path_parameter(
            &doc,
            "/api/v1/regressions/{artifact_id}",
            "get",
            "artifact_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/regressions/trends/{scenario_id}",
            "get",
            "scenario_id",
        );
        assert_path_parameter(&doc, "/api/v1/workflow-runs/{run_id}", "get", "run_id");
        assert_path_parameter(
            &doc,
            "/api/v1/workflow-runs/{run_id}/events",
            "get",
            "run_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/workflow-runs/{run_id}/approvals",
            "get",
            "run_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/workflow-runs/{run_id}/resume",
            "post",
            "run_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/workflow-runs/{run_id}/cancel",
            "post",
            "run_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/workflow-runs/{run_id}/tick",
            "post",
            "run_id",
        );
        assert_path_parameter(&doc, "/api/v1/team/{user_id}", "put", "user_id");
        assert_path_parameter(&doc, "/api/v1/team/{user_id}", "delete", "user_id");
        assert_path_parameter(&doc, "/api/v1/backups/{backup_id}", "delete", "backup_id");
        assert_path_parameter(&doc, "/api/v1/keys/{key_id}/revoke", "post", "key_id");
        assert_path_parameter(&doc, "/api/v1/keys/{key_id}/rotate", "post", "key_id");
        assert_path_parameter(&doc, "/api/v1/keys/{key_id}", "delete", "key_id");
        assert_path_parameter(&doc, "/api/v1/keys/{key_id}/scopes", "post", "key_id");
        assert_path_parameter(
            &doc,
            "/api/v1/backups/{backup_id}/restore",
            "post",
            "backup_id",
        );
        assert_path_parameter(&doc, "/api/v1/operator/evidence/{run_id}", "get", "run_id");
        assert_path_parameter(
            &doc,
            "/api/v1/tool-policy/capabilities/{tool_name}",
            "put",
            "tool_name",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/tool-policy/profiles/{profile_id}/allowlist",
            "put",
            "profile_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/tool-policy/hooks/{hook_id}",
            "put",
            "hook_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/memories/{memory_id}/supersede",
            "post",
            "memory_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/memories/{memory_id}/reembed",
            "post",
            "memory_id",
        );
        assert_path_parameter(
            &doc,
            "/api/v1/memories/{memory_id}/embedding/reconcile",
            "post",
            "memory_id",
        );
        for path in [
            "/api/v1/product/tasks/{task_id}/delegated/reconcile-unadmitted",
            "/api/v1/product/tasks/{task_id}/delegated/prepare",
            "/api/v1/product/tasks/{task_id}/delegated/activate",
            "/api/v1/product/tasks/{task_id}/delegated/approve",
            "/api/v1/product/tasks/{task_id}/delegated/terminal",
        ] {
            assert_path_parameter(&doc, path, "post", "task_id");
        }
    }

    #[test]
    fn test_openapi_mutation_routes_document_request_bodies() {
        let doc = openapi_document();

        assert_required_body_fields(&doc, "/api/v1/plans", "post", &["raw_request"]);
        assert_required_body_fields(&doc, "/api/v1/workflow-runs", "post", &["plan_id"]);
        assert_required_body_fields(&doc, "/api/v1/workflow-runs/{run_id}/tick", "post", &[]);
        assert_required_body_fields(
            &doc,
            "/api/v1/workflow-runs/{run_id}/events",
            "post",
            &["event_type"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/workflow-runs/{run_id}/approvals",
            "post",
            &["node_id", "decision"],
        );
        assert_required_body_fields(&doc, "/api/v1/team/{user_id}", "put", &["role"]);
        assert_required_body_fields(&doc, "/api/v1/backups", "post", &["confirm_local_backup"]);
        assert_required_body_fields(
            &doc,
            "/api/v1/import",
            "post",
            &["snapshot", "confirm_import"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/backups/{backup_id}/restore",
            "post",
            &["confirm_restore"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories",
            "post",
            &[
                "scope",
                "run_id",
                "source_id",
                "source_sha256",
                "conflict_key",
                "content",
                "confidence",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories/{memory_id}/supersede",
            "post",
            &[
                "run_id",
                "scope",
                "winner_expected_version",
                "loser_memory_id",
                "loser_expected_version",
                "confirm_supersede",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories/{memory_id}/reembed",
            "post",
            &["run_id", "scope", "expected_version", "confirm_reembed"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories/{memory_id}/embedding/reconcile",
            "post",
            &[
                "run_id",
                "scope",
                "target_version",
                "expected_attempt_count",
                "action",
                "confirm_resolution",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories/prune",
            "post",
            &["run_id", "scope", "confirm_prune"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/memories/retrieve",
            "post",
            &[
                "scope",
                "run_id",
                "node_id",
                "query",
                "top_k",
                "max_tokens",
                "max_bytes",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/offline-replays/production-profile",
            "put",
            &["profile", "confirm_profile"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/product/tasks/{task_id}/delegated/reconcile-unadmitted",
            "post",
            &["delegation_id"],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/product/tasks/{task_id}/delegated/prepare",
            "post",
            &[
                "delegation",
                "proposal_manifest",
                "approved_proposal_sha256",
                "attempt_id",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/product/tasks/{task_id}/delegated/activate",
            "post",
            &[
                "delegation_id",
                "attempt_id",
                "final_manifest",
                "spend_authorization_id",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/product/tasks/{task_id}/delegated/approve",
            "post",
            &[
                "expected_task_version",
                "delegation_id",
                "final_manifest",
                "current_target_main_sha",
            ],
        );
        assert_required_body_fields(
            &doc,
            "/api/v1/product/tasks/{task_id}/delegated/terminal",
            "post",
            &["delegation_id", "attempt_id"],
        );
    }

    fn assert_path_parameter(doc: &Value, path: &str, method: &str, name: &str) {
        let params = doc["paths"][path][method]["parameters"]
            .as_array()
            .expect("dynamic route must document path parameters");
        let param = params
            .iter()
            .find(|param| param["name"] == name)
            .expect("expected named path parameter");
        assert_eq!(param["in"], "path");
        assert_eq!(param["required"], true);
        assert_eq!(param["schema"]["type"], "string");
    }

    fn assert_required_body_fields(doc: &Value, path: &str, method: &str, fields: &[&str]) {
        let request_body = &doc["paths"][path][method]["requestBody"];
        assert_eq!(request_body["required"], true);
        let required = request_body["content"]["application/json"]["schema"]["required"]
            .as_array()
            .expect("request body required fields must be documented");
        for field in fields {
            assert!(
                required.iter().any(|item| item == field),
                "{path} {method} must require {field}"
            );
        }
    }

    #[test]
    fn test_openapi_decisions_routes_documented() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().expect("paths should be an object");

        assert!(
            paths.contains_key("/api/v1/decisions"),
            "OpenAPI document must include /api/v1/decisions"
        );
        assert!(
            paths.contains_key("/api/v1/decisions/stats"),
            "OpenAPI document must include /api/v1/decisions/stats"
        );
        assert!(
            paths.contains_key("/api/v1/decisions/{decision_id}"),
            "OpenAPI document must include /api/v1/decisions/{{decision_id}}"
        );

        assert_path_parameter(
            &doc,
            "/api/v1/decisions/{decision_id}",
            "get",
            "decision_id",
        );
    }
}
