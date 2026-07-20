use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::env;

use crate::http_server::middleware::{
    authorize, backup_dir_for_state, cors_headers, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{openapi_document, AXUM_API_SCHEMA_VERSION};
use crate::infrastructure::resource_monitor;
use crate::storage::backup_manager::BackupManager;

pub(crate) async fn api_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;

    let disk_warn_pct: f64 = env::var("ACP_DISK_WARN_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);
    let mem_warn_pct: f64 = env::var("ACP_MEM_WARN_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);

    let mut has_error = false;
    let mut has_degraded = false;

    // DB integrity check
    let db_check = if let Some(store) = &state.local_store {
        match store.check_integrity() {
            Ok(report) if report.status == "ok" => {
                let file_size = if !store.is_memory() {
                    resource_monitor::db_file_size(store.db_path()).ok()
                } else {
                    None
                };
                let mut entry = json!({
                    "status": "ok",
                    "integrity": "ok",
                });
                if let Some(sz) = file_size {
                    entry["file_size_bytes"] = json!(sz);
                }
                entry
            }
            Ok(report) => {
                has_error = true;
                json!({
                    "status": "error",
                    "integrity": report.status,
                })
            }
            Err(e) => {
                has_error = true;
                json!({
                    "status": "error",
                    "integrity": "error",
                    "error": e,
                })
            }
        }
    } else {
        json!({ "status": "unavailable" })
    };

    // Disk check
    let disk_check = match resource_monitor::disk_usage("/") {
        Ok(usage) => {
            let free_pct = 100.0 - usage.usage_pct;
            let status = if free_pct < disk_warn_pct {
                has_degraded = true;
                "degraded"
            } else {
                "ok"
            };
            json!({
                "status": status,
                "free_bytes": usage.free_bytes,
                "total_bytes": usage.total_bytes,
                "usage_pct": (usage.usage_pct * 100.0).round() / 100.0,
            })
        }
        Err(e) => {
            has_degraded = true;
            json!({
                "status": "unknown",
                "error": e,
            })
        }
    };

    // Memory check (Linux probe). On platforms without a memory probe, report
    // unsupported without degrading overall health — DB integrity remains the
    // hard readiness signal for the public no-provider demo path.
    let memory_check = match resource_monitor::memory_usage() {
        Ok(mem) => {
            let free_pct = 100.0 - mem.usage_pct;
            let status = if free_pct < mem_warn_pct {
                has_degraded = true;
                "degraded"
            } else {
                "ok"
            };
            json!({
                "status": status,
                "available_bytes": mem.available_bytes,
                "total_bytes": mem.total_bytes,
                "usage_pct": (mem.usage_pct * 100.0).round() / 100.0,
            })
        }
        Err(e) => {
            if resource_monitor::memory_probe_is_platform_unsupported(&e) {
                json!({
                    "status": "unsupported",
                    "error": e,
                })
            } else {
                has_degraded = true;
                json!({
                    "status": "unknown",
                    "error": e,
                })
            }
        }
    };

    // Scheduler check (preserves existing logic)
    let mut scheduler_persisted = false;
    let scheduler_check = if let Some(sched) = &state.scheduler {
        if let Ok(guard) = sched.lock() {
            let status = guard.status();
            if let Some(last_tick) = status.get("last_tick_at").and_then(|v| v.as_str()) {
                let ok = chrono::NaiveDateTime::parse_from_str(last_tick, "%Y-%m-%dT%H:%M:%SZ")
                    .ok()
                    .map(|t| {
                        let now = chrono::Utc::now().naive_utc();
                        let diff = now.signed_duration_since(t);
                        diff.num_seconds() < 30
                    })
                    .unwrap_or(false);
                if !ok {
                    has_degraded = true;
                }
                json!({
                    "status": if ok { "ok" } else { "stale" },
                    "last_heartbeat_at": last_tick,
                    "persisted": false,
                })
            } else {
                // Try persisted heartbeat
                let (ok, ts) = state
                    .local_store
                    .as_ref()
                    .and_then(|store| store.read_heartbeat().ok().flatten())
                    .and_then(|row| {
                        if row.last_heartbeat_at.is_empty() {
                            return None;
                        }
                        chrono::NaiveDateTime::parse_from_str(
                            &row.last_heartbeat_at,
                            "%Y-%m-%dT%H:%M:%SZ",
                        )
                        .ok()
                        .map(|t| {
                            scheduler_persisted = true;
                            let now = chrono::Utc::now().naive_utc();
                            let diff = now.signed_duration_since(t);
                            (diff.num_seconds() < 30, row.last_heartbeat_at.clone())
                        })
                    })
                    .unwrap_or((false, String::new()));
                if !ok {
                    has_degraded = true;
                }
                json!({
                    "status": if ok { "ok" } else { "stale" },
                    "last_heartbeat_at": if ts.is_empty() { serde_json::Value::Null } else { json!(ts) },
                    "persisted": scheduler_persisted,
                })
            }
        } else {
            has_degraded = true;
            json!({
                "status": "stale",
                "error": "scheduler lock poisoned",
            })
        }
    } else {
        json!({
            "status": "unavailable",
        })
    };

    // Backup check
    let backup_check = if let Some(store) = &state.local_store {
        if !store.is_memory() {
            let backup_dir = backup_dir_for_state(&state, store.db_path());
            match BackupManager::new(&backup_dir) {
                Ok(manager) => match manager.list_backups() {
                    Ok(backups) => {
                        if let Some(latest) = backups
                            .iter()
                            .max_by(|a, b| a.created_at.cmp(&b.created_at))
                        {
                            let age_seconds = chrono::Utc::now()
                                .naive_utc()
                                .signed_duration_since(
                                    chrono::NaiveDateTime::parse_from_str(
                                        &latest.created_at,
                                        "%Y-%m-%dT%H:%M:%SZ",
                                    )
                                    .unwrap_or_else(|_| chrono::Utc::now().naive_utc()),
                                )
                                .num_seconds()
                                .max(0);
                            let status = if age_seconds > 86400 {
                                has_degraded = true;
                                "degraded"
                            } else {
                                "ok"
                            };
                            json!({
                                "status": status,
                                "last_backup_at": latest.created_at,
                                "age_seconds": age_seconds,
                            })
                        } else {
                            json!({
                                "status": "unavailable",
                                "last_backup_at": null,
                                "age_seconds": null,
                            })
                        }
                    }
                    Err(e) => json!({
                        "status": "unknown",
                        "error": e,
                    }),
                },
                Err(e) => json!({
                    "status": "unknown",
                    "error": e,
                }),
            }
        } else {
            json!({ "status": "unavailable" })
        }
    } else {
        json!({ "status": "unavailable" })
    };

    let overall = if has_error {
        "unhealthy"
    } else if has_degraded {
        "degraded"
    } else {
        "healthy"
    };

    let report = json!({
        "schema_version": AXUM_API_SCHEMA_VERSION,
        "status": overall,
        "checks": {
            "db": db_check,
            "disk": disk_check,
            "memory": memory_check,
            "scheduler": scheduler_check,
            "backup": backup_check,
        },
        "tenant_id": context.tenant_id,
        "request_id": context.request_id,
    });

    // Fire-and-forget webhook alert if status degraded/unhealthy
    if overall != "healthy" {
        if let Ok(webhook_url) = env::var("ACP_HEALTH_ALERT_WEBHOOK_URL") {
            if !webhook_url.is_empty() {
                let payload = report.clone();
                tokio::spawn(async move {
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(2))
                        .build();
                    if let Ok(client) = client {
                        let _ = client.post(&webhook_url).json(&payload).send().await;
                    }
                });
            }
        }
    }

    Ok((cors_headers(), Json(report)))
}

pub(crate) async fn api_ready(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "ready",
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
        })),
    ))
}

pub(crate) async fn api_openapi(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    Ok((cors_headers(), Json(openapi_document())))
}
