pub(crate) mod handlers;
pub(crate) mod middleware;
pub(crate) mod routes;
pub(crate) mod server_context;
pub(crate) mod state;

pub use routes::{build_axum_router, build_axum_router_with_dashboard};
pub use server_context::{RouteHandler, RouteMatch, ServerContext};
pub use state::{AxumApiState, ServerConfig};

pub const HTTP_SERVER_SCHEMA_VERSION: &str = "http_server.v1";
pub const AXUM_API_SCHEMA_VERSION: &str = "axum_api.v1";
pub const MAX_BODY_SIZE: usize = 1_048_576;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DispatchApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReadOnlyPlanApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
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
    json!({
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
            "/api/v1/plans": {
                "get": {
                    "summary": "List persisted read-only workflow plans",
                    "description": "Requires dispatch:read scope. Plans are app-owned metadata only and do not execute workers or write target repositories.",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 100, "minimum": 0, "maximum": 500}},
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}},
                        {"name": "search", "in": "query", "schema": {"type": "string"}, "description": "Case-insensitive match across plan id, request text, source, status, workflow id, and dispatch id."}
                    ],
                    "responses": {"200": {"description": "Read-only workflow plan list"}}
                },
                "post": {
                    "summary": "Create a read-only workflow plan",
                    "description": "Generates a canonical WorkflowGraph plan only. No execution, provider call, worker spawn, sandbox/process execution, target write, deploy, merge, or approval control is performed.",
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
                    "description": "Requires dispatch:read scope. Returns app-owned planning metadata only.",
                    "parameters": [path_parameter("plan_id")],
                    "responses": {
                        "200": {"description": "Read-only workflow plan"},
                        "404": {"description": "Plan not found"}
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
            "/api/v1/provider/health": {
                "get": {
                    "summary": "Provider health check",
                    "description": "Reports provider status: noop if no provider configured, ok if enabled, error if disabled or unavailable.",
                    "responses": {
                        "200": {"description": "Provider health status"}
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
    })
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
}
