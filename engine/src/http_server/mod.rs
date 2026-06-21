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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReadOnlyPlanApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkflowRunCreateApiRequest {
    pub plan_id: String,
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
pub struct SupervisedPatchWorkspaceVerifyRequest {
    pub command: String,
    pub confirm_verification: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub attempt: Option<u64>,
    pub repair_executor: Option<String>,
    pub max_repair_attempts: Option<u64>,
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
                    "description": "Generates a canonical WorkflowGraph plan with recommendation-only quality/routing/retry/observability advisory metadata. No execution, provider call, worker spawn, sandbox/process execution, target write, deploy, merge, or approval control is performed.",
                    "requestBody": json_request_body(&["raw_request"], json!({
                        "raw_request": {"type": "string"},
                        "request_source": {"type": "string", "default": "api"}
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
                    "summary": "Create inert workflow run metadata from a read-only plan",
                    "description": "Persists run/node/edge/event metadata only. It does not execute, resume execution, spawn workers, call providers, or write target repositories.",
                    "requestBody": json_request_body(&["plan_id"], json!({
                        "plan_id": {"type": "string"}
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
                    "description": "Finds a ready node (all predecessors completed), leases it, executes via noop/stub, and records the result. Returns the tick result with node execution details. Returns 409 if the run is already terminal.",
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
                        "max_repair_attempts": {"type": "integer", "minimum": 1, "maximum": 2}
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
                    "description": "Requires backup:admin scope and confirm_restore=true. Restores from backup, runs integrity check, reports row counts.",
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
    append_adaptive_fusion_openapi_paths(&mut doc);
    doc
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
                "summary": "Promote one adaptive fusion policy",
                "description": "Requires configured auth, team:admin scope, confirm_adaptive_policy_promotion=true, ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION=1, ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE=1, minimum evidence, local evidence IDs, and non-regressing metrics.",
                "requestBody": json_request_body(&["promotion"], json!({
                    "actor": {"type": "string"},
                    "promotion": {"type": "object"}
                })),
                "responses": {"200": {"description": "Adaptive fusion promotion result"}}
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
