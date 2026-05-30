use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    CreateTeamMemberRequest, UpdateMemberRoleRequest, AXUM_API_SCHEMA_VERSION,
};

pub(crate) async fn api_team(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "team:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(store.team_snapshot().map_err(internal_error)?),
    ))
}

pub(crate) async fn api_create_member(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<CreateTeamMemberRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    store
        .upsert_team_member(&request.user_id, &request.display_name, &request.role)
        .map_err(internal_error)?;

    store
        .append_audit(
            &context.api_key_id,
            "team.member.created",
            &request.user_id,
            &json!({"user_id": request.user_id, "display_name": request.display_name, "role": request.role}),
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": request.user_id,
            "display_name": request.display_name,
            "role": request.role,
        })),
    ))
}

pub(crate) async fn api_update_member_role(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(user_id): AxumPath<String>,
    Json(request): Json<UpdateMemberRoleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let updated = store
        .update_team_member_role(&user_id, &request.role, &context.api_key_id)
        .map_err(internal_error)?;

    if !updated {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "member not found",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": user_id,
            "role": request.role,
        })),
    ))
}

pub(crate) async fn api_delete_member(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(user_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let deleted = store
        .delete_team_member(&user_id, &context.api_key_id)
        .map_err(internal_error)?;

    if !deleted {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "member not found",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": user_id,
        })),
    ))
}
