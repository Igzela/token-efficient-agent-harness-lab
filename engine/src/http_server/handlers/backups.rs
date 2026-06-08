use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, backup_dir_for_state, cors_headers, internal_error, require_store, ApiError,
    RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    BackupApiRequest, RestoreApiRequest, RestoreDryRunApiRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::storage::backup_manager::BackupManager;

pub(crate) async fn api_list_backups(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for backups",
        ));
    }
    authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let backups = manager.list_backups().map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "backups": backups,
        })),
    ))
}

pub(crate) async fn api_create_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<BackupApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for local backup",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    if request.confirm_local_backup != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "backup_confirmation_required",
            "confirm_local_backup must be true",
        ));
    }
    let store = require_store(&state)?;
    if store.is_memory() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "file_store_required",
            "file-backed local store is required for backup",
        ));
    }
    if store.is_postgres() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "backup_not_supported",
            "PostgreSQL mode: use pg_dump or managed backup. App file-copy backup is not available for PostgreSQL backends.",
        ));
    }
    store.checkpoint_wal().map_err(internal_error)?;

    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let mut backups = manager.list_backups().map_err(internal_error)?;
    let backup_id = format!("backup-{:04}", backups.len() + 1);
    let label = request.label.as_deref().unwrap_or("manual");
    let now_iso = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let backup = manager
        .create_backup(store.db_path(), label, &backup_id, &now_iso)
        .map_err(internal_error)?;
    backups.push(backup.clone());
    manager.save_metadata(&backups).map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "backup.create",
            &backup.backup_id,
            &json!({"label": label, "backup_path": backup.backup_path}),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "backup": backup})),
    ))
}

pub(crate) async fn api_delete_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(backup_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for backups",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let deleted = manager.delete_backup(&backup_id).map_err(internal_error)?;
    if !deleted {
        return Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "backup_not_found",
            "backup not found",
        ));
    }
    store
        .append_audit(
            &context.api_key_id,
            "backup.delete",
            &backup_id,
            &json!({"backup_id": backup_id}),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(
            json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "backup_id": backup_id}),
        ),
    ))
}

pub(crate) async fn api_verify_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(backup_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for backup verification",
        ));
    }
    authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let verification = manager.verify_backup(&backup_id).map_err(|e| {
        if e.starts_with("backup not found:") {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "backup_not_found",
                "backup not found",
            )
        } else {
            internal_error(e)
        }
    })?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "verification": verification,
        })),
    ))
}

pub(crate) async fn api_restore_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(backup_id): AxumPath<String>,
    Json(request): Json<RestoreApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for restore",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    if request.confirm_restore != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "restore_confirmation_required",
            "confirm_restore must be true",
        ));
    }
    let store = require_store(&state)?;
    if store.is_memory() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "file_store_required",
            "file-backed local store is required for restore",
        ));
    }
    if store.is_postgres() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "backup_not_supported",
            "PostgreSQL mode: use pg_restore or managed backup. App file-copy restore is not available for PostgreSQL backends.",
        ));
    }
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let result = manager
        .restore_backup_with_verify(&backup_id, store.db_path(), state.now)
        .map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "backup.restore",
            &backup_id,
            &json!({
                "success": result.success,
                "records_restored": result.records_restored,
                "errors": result.errors,
            }),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "restore": {
                "success": result.success,
                "records_restored": result.records_restored,
                "errors": result.errors,
                "duration_ms": result.duration_ms,
            },
        })),
    ))
}

pub(crate) async fn api_restore_backup_dry_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(backup_id): AxumPath<String>,
    Json(request): Json<RestoreDryRunApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "backup_admin_required",
            "admin auth is required for restore dry-run",
        ));
    }
    authorize(&state, &headers, "backup:admin", uri.path(), &request_id.0)?;
    if request.confirm_restore_dry_run != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "restore_dry_run_confirmation_required",
            "confirm_restore_dry_run must be true",
        ));
    }
    let store = require_store(&state)?;
    if store.is_memory() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "file_store_required",
            "file-backed local store is required for restore dry-run",
        ));
    }
    if store.is_postgres() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "backup_not_supported",
            "PostgreSQL mode: use pg_dump/pg_restore or managed backup. App file-copy backup is not available for PostgreSQL backends.",
        ));
    }
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let verification = manager
        .restore_dry_run(&backup_id, store.db_path())
        .map_err(|e| {
            if e.starts_with("backup not found:") {
                ApiError::with_code(
                    StatusCode::NOT_FOUND,
                    "backup_not_found",
                    "backup not found",
                )
            } else {
                internal_error(e)
            }
        })?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "restore_dry_run": verification,
        })),
    ))
}
