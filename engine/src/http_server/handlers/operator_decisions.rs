use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::http_server::middleware::{authorize, cors_headers, require_store, ApiError, RequestId};
use crate::http_server::state::AxumApiState;

const DEFAULT_FRESHNESS_SECONDS: u64 = 300;

#[derive(Debug, Deserialize)]
pub(crate) struct OperatorDecisionQueueQuery {
    generated_at: Option<String>,
    maximum_freshness_seconds: Option<u64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(crate) async fn api_operator_decisions(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<OperatorDecisionQueueQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let generated_at = query
        .generated_at
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let freshness = query
        .maximum_freshness_seconds
        .unwrap_or(DEFAULT_FRESHNESS_SECONDS);
    if freshness == 0 || freshness > 30 * 24 * 60 * 60 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_operator_decision_query",
            "maximum_freshness_seconds must be between 1 and 2592000",
        ));
    }
    let queue = require_store(&state)?
        .operator_decision_queue(
            &generated_at,
            freshness,
            query.limit.unwrap_or(50),
            query.offset.unwrap_or(0),
        )
        .map_err(|error| {
            ApiError::with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "operator_decision_queue_unavailable",
                error,
            )
        })?;
    Ok((
        cors_headers(),
        Json(json!({
            "read_only": true,
            "metadata_only": true,
            "mutation_authority": "none",
            "provider_calls": "disabled",
            "target_repository_writes": "disabled",
            "queue": queue,
        })),
    ))
}
