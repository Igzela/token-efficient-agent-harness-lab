use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::Router;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::handlers::*;
use super::middleware::{cors_layer, cors_preflight, request_id_layer};
use super::state::AxumApiState;

pub fn build_axum_router(state: AxumApiState) -> Router {
    axum_routes()
        .layer(axum::middleware::from_fn(cors_layer))
        .layer(axum::middleware::from_fn(request_id_layer))
        .with_state(state)
}

pub fn build_axum_router_with_dashboard(
    state: AxumApiState,
    dashboard_dir: impl Into<PathBuf>,
) -> Router {
    axum_routes()
        .fallback(serve_dashboard_asset)
        .layer(axum::middleware::from_fn(cors_layer))
        .layer(axum::middleware::from_fn(request_id_layer))
        .with_state(state.with_dashboard_dir(dashboard_dir))
}

fn axum_routes() -> Router<AxumApiState> {
    Router::new()
        .route(
            "/api/v1/health",
            get(health::api_health).options(cors_preflight),
        )
        .route(
            "/api/v1/ready",
            get(health::api_ready).options(cors_preflight),
        )
        .route(
            "/api/v1/openapi.json",
            get(health::api_openapi).options(cors_preflight),
        )
        .route(
            "/api/v1/metrics",
            get(operations::api_metrics).options(cors_preflight),
        )
        .route(
            "/api/v1/metrics/observability",
            get(operations::api_observability_metrics).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatch",
            post(dispatch::api_dispatch).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatches",
            get(dispatch::api_dispatches).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatches/:dispatch_id",
            get(dispatch::api_dispatch_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatch-metrics",
            get(dispatch::api_dispatch_metrics).options(cors_preflight),
        )
        .route(
            "/api/v1/feedback/traces",
            get(dispatch::api_feedback_traces).options(cors_preflight),
        )
        .route(
            "/api/v1/feedback/patterns",
            get(dispatch::api_feedback_patterns).options(cors_preflight),
        )
        .route(
            "/api/v1/feedback/cost-of-pass",
            get(dispatch::api_feedback_cost_of_pass).options(cors_preflight),
        )
        .route(
            "/api/v1/simulation/report",
            get(dispatch::api_simulation_report).options(cors_preflight),
        )
        .route(
            "/api/v1/simulation/policy-delta",
            get(dispatch::api_policy_simulation_report).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals",
            get(dispatch::api_policy_proposals)
                .post(dispatch::api_create_policy_proposal)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/:proposal_id",
            get(dispatch::api_policy_proposal_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/:proposal_id/approve",
            post(dispatch::api_approve_policy_proposal).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/:proposal_id/reject",
            post(dispatch::api_reject_policy_proposal).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/:proposal_id/deactivate",
            post(dispatch::api_deactivate_policy_proposal).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/:proposal_id/rollback",
            post(dispatch::api_rollback_policy_proposal).options(cors_preflight),
        )
        .route(
            "/api/v1/proposals/generated",
            get(dispatch::api_generated_proposals).options(cors_preflight),
        )
        .route(
            "/api/v1/auto-adjustments",
            get(dispatch::api_auto_adjustments).options(cors_preflight),
        )
        .route(
            "/api/v1/plans",
            get(plans::api_plans)
                .post(plans::api_create_plan)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/plans/:plan_id",
            get(plans::api_plan_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs",
            get(workflow_runs::api_workflow_runs)
                .post(workflow_runs::api_create_workflow_run)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id",
            get(workflow_runs::api_workflow_run_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id/events",
            get(workflow_runs::api_workflow_run_events)
                .post(workflow_runs::api_create_workflow_run_event)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id/approvals",
            get(workflow_runs::api_workflow_run_approvals)
                .post(workflow_runs::api_create_workflow_run_approval)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id/resume",
            post(workflow_runs::api_resume_workflow_run).options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id/cancel",
            post(workflow_runs::api_cancel_workflow_run).options(cors_preflight),
        )
        .route(
            "/api/v1/workflow-runs/:run_id/tick",
            post(workflow_runs::api_tick_workflow_run).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/workspaces",
            get(supervised_patch::api_supervised_patch_workspaces)
                .post(supervised_patch::api_create_supervised_patch_workspace)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/workspaces/:workspace_id",
            get(supervised_patch::api_supervised_patch_workspace_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/workspaces/:workspace_id/cleanup",
            post(supervised_patch::api_cleanup_supervised_patch_workspace).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/workspaces/:workspace_id/quarantine",
            post(supervised_patch::api_quarantine_supervised_patch_workspace)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/artifacts",
            get(supervised_patch::api_supervised_patch_artifacts).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/workspaces/:workspace_id/capture",
            post(supervised_patch::api_capture_supervised_patch).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/artifacts/:artifact_id",
            get(supervised_patch::api_supervised_patch_artifact_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/supervised-patch/artifacts/:artifact_id/export",
            post(supervised_patch::api_export_supervised_patch).options(cors_preflight),
        )
        .route(
            "/api/v1/dashboard",
            get(dashboard::api_dashboard).options(cors_preflight),
        )
        .route(
            "/api/v1/config",
            get(data_ops::api_config).options(cors_preflight),
        )
        .route(
            "/api/v1/team",
            get(team::api_team)
                .post(team::api_create_member)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/team/:user_id",
            put(team::api_update_member_role)
                .delete(team::api_delete_member)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/costs",
            get(costs::api_costs).options(cors_preflight),
        )
        .route(
            "/api/v1/costs/dispatches",
            get(costs::api_cost_details).options(cors_preflight),
        )
        .route(
            "/api/v1/export",
            get(data_ops::api_export).options(cors_preflight),
        )
        .route(
            "/api/v1/audit",
            get(audit::api_audit).options(cors_preflight),
        )
        .route(
            "/api/v1/backups",
            get(backups::api_list_backups)
                .post(backups::api_create_backup)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/backups/:backup_id",
            delete(backups::api_delete_backup).options(cors_preflight),
        )
        .route(
            "/api/v1/backups/:backup_id/verify",
            get(backups::api_verify_backup).options(cors_preflight),
        )
        .route(
            "/api/v1/keys",
            get(keys::api_list_keys)
                .post(keys::api_create_key)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/revoke",
            post(keys::api_revoke_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/rotate",
            post(keys::api_rotate_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id",
            delete(keys::api_delete_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/scopes",
            post(keys::api_update_key_scopes).options(cors_preflight),
        )
        .route(
            "/api/v1/scheduler/status",
            get(scheduler::api_scheduler_status).options(cors_preflight),
        )
        .route(
            "/api/v1/executor-pool",
            get(executor_pool::api_executor_pool_status).options(cors_preflight),
        )
        .route(
            "/api/v1/queue/status",
            get(queue::api_queue_status).options(cors_preflight),
        )
        .route(
            "/api/v1/queue/runs",
            get(queue::api_queue_runs).options(cors_preflight),
        )
        .route(
            "/api/v1/queue/runs/:run_id/priority",
            put(queue::api_update_run_priority).options(cors_preflight),
        )
        .route(
            "/api/v1/queue/runs/:run_id/pause",
            put(queue::api_update_run_pause).options(cors_preflight),
        )
        .route(
            "/api/v1/queue/tenants",
            get(queue::api_queue_tenants).options(cors_preflight),
        )
        .route(
            "/api/v1/decisions",
            get(decisions::api_decisions).options(cors_preflight),
        )
        .route(
            "/api/v1/decisions/stats",
            get(decisions::api_decision_stats).options(cors_preflight),
        )
        .route(
            "/api/v1/decisions/:decision_id",
            get(decisions::api_decision_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/provider/health",
            get(provider::api_provider_health).options(cors_preflight),
        )
        .route(
            "/api/v1/provider/audit",
            get(provider::api_provider_audit).options(cors_preflight),
        )
        .route(
            "/api/v1/storage/integrity",
            get(data_ops::api_integrity).options(cors_preflight),
        )
        .route(
            "/api/v1/import",
            post(data_ops::api_import).options(cors_preflight),
        )
        .route(
            "/api/v1/backups/:backup_id/restore",
            post(backups::api_restore_backup).options(cors_preflight),
        )
        .route(
            "/api/v1/backups/:backup_id/restore/dry-run",
            post(backups::api_restore_backup_dry_run).options(cors_preflight),
        )
        .route(
            "/api/v1/circuit-breaker/status",
            get(operations::api_circuit_breaker_status).options(cors_preflight),
        )
}

async fn serve_dashboard_asset(State(state): State<AxumApiState>, uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let Some(dashboard_dir) = &state.dashboard_dir else {
        return (StatusCode::NOT_FOUND, "dashboard not configured").into_response();
    };
    let Some(path) = dashboard_asset_path(dashboard_dir.as_ref(), uri.path()) else {
        return (StatusCode::BAD_REQUEST, "invalid dashboard path").into_response();
    };

    match fs::read(&path) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for_path(&path)),
            );
            (headers, bytes).into_response()
        }
        Err(_) => match fs::read(dashboard_dir.join("index.html")) {
            Ok(bytes) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                );
                (headers, bytes).into_response()
            }
            Err(_) => (StatusCode::NOT_FOUND, "dashboard asset not found").into_response(),
        },
    }
}

fn dashboard_asset_path(root: &Path, uri_path: &str) -> Option<PathBuf> {
    let relative = uri_path.trim_start_matches('/');
    if relative.is_empty() {
        return Some(root.join("index.html"));
    }

    let mut path = PathBuf::from(root);
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(if path.is_dir() {
        path.join("index.html")
    } else {
        path
    })
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "ico" => "image/x-icon",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}
