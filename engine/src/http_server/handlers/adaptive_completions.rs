use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;

use crate::http_server::middleware::{authorize, cors_headers, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::http_server::AdaptiveFusionCompletionApiRequest;

mod completion;
pub(crate) use completion::execute_adaptive_completion;

pub(crate) async fn api_adaptive_completion(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<AdaptiveFusionCompletionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    let response =
        execute_adaptive_completion(&state, request, &request_id.0, &context.api_key_id).await?;
    Ok((cors_headers(), Json(response)))
}
