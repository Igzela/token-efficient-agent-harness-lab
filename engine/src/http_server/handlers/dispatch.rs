use axum::extract::{Path as AxumPath, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{DispatchApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::provider::cost_gate::{check_cost_gates, CostGateConfig};

pub(crate) async fn api_dispatch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<DispatchApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "raw_request is required",
        ));
    }

    let is_provider = state.executor_type() == "provider";
    if is_provider {
        authorize(&state, &headers, "dispatch:execute")?;
    }

    let request_source = request.request_source.as_deref().unwrap_or("api");

    if is_provider {
        let cost_config = CostGateConfig::from_env();
        if cost_config.is_active() {
            let reserved = state
                .engine
                .preflight_reserved_cost(&request.raw_request, request_source);
            let daily_cost = if let Some(store) = &state.local_store {
                let today_prefix = &crate::http_server::middleware::chrono_free_today()[..10];
                store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
            } else {
                0.0
            };
            if check_cost_gates(&cost_config, reserved, daily_cost).is_err() {
                let raw = request.raw_request.clone();
                let src = request_source.to_string();
                let eng = Arc::clone(&state.engine);
                let bundle = tokio::task::spawn_blocking(move || eng.dispatch(&raw, &src))
                    .await
                    .map_err(|e| internal_error(e.to_string()))?;
                if let Some(store) = &state.local_store {
                    store
                        .record_dispatch(
                            &request.raw_request,
                            request_source,
                            &bundle,
                            &context.api_key_id,
                        )
                        .map_err(internal_error)?;
                }
                return Ok((cors_headers(), Json(bundle)));
            }
        }
    }

    let raw = request.raw_request.clone();
    let src = request_source.to_string();
    let eng = Arc::clone(&state.engine);
    let bundle = tokio::task::spawn_blocking(move || eng.dispatch(&raw, &src))
        .await
        .map_err(|e| internal_error(e.to_string()))?;
    if let Some(store) = &state.local_store {
        store
            .record_dispatch(
                &request.raw_request,
                request_source,
                &bundle,
                &context.api_key_id,
            )
            .map_err(internal_error)?;
    }
    Ok((cors_headers(), Json(bundle)))
}

pub(crate) async fn api_dispatches(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let search = params.get("search").map(String::as_str);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "dispatches": store.search_dispatches(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_dispatch_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(dispatch_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store.get_dispatch(&dispatch_id).map_err(internal_error)? {
        Some(dispatch) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "dispatch": dispatch,
            })),
        )),
        None => Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "dispatch not found",
        )),
    }
}
