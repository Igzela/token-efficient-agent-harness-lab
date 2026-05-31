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
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DispatchApiRequest {
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
                    "responses": {
                        "200": {"description": "Dispatch detail"},
                        "404": {"description": "Dispatch not found"}
                    }
                }
            },
            "/api/v1/dashboard": {
                "get": {
                    "summary": "Read local dashboard state from SQLite-backed runtime state",
                    "responses": {"200": {"description": "Dashboard state"}}
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
                    "responses": {
                        "200": {"description": "Member updated"},
                        "404": {"description": "Member not found"}
                    }
                },
                "delete": {
                    "summary": "Remove a team member",
                    "description": "Requires team:admin scope.",
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
                        {"name": "offset", "in": "query", "schema": {"type": "integer", "default": 0, "minimum": 0}}
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
                    "responses": {
                        "200": {"description": "Backup deleted"},
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
                    "responses": {
                        "200": {"description": "Restore result"},
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
}
