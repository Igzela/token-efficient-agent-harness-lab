use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::infrastructure::auth::AuthDecision;
use crate::storage::local_product_store::LocalProductStore;

use super::state::AxumApiState;
use super::AXUM_API_SCHEMA_VERSION;

#[derive(Debug, Clone)]
pub(crate) struct ApiRequestContext {
    pub tenant_id: String,
    pub api_key_id: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct ApiErrorBody {
    code: String,
    error: String,
    schema_version: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ApiError {
    pub code: String,
    pub status: StatusCode,
    pub error: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, error: impl Into<String>) -> Self {
        Self {
            code: default_error_code(status).to_string(),
            status,
            error: error.into(),
        }
    }

    pub(crate) fn with_code(
        status: StatusCode,
        code: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            status,
            error: error.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            cors_headers(),
            Json(ApiErrorBody {
                code: self.code,
                error: self.error,
                schema_version: AXUM_API_SCHEMA_VERSION.to_string(),
            }),
        )
            .into_response()
    }
}

const HEALTH_PATHS: &[&str] = &["/api/v1/health", "/api/v1/ready"];

fn is_health_path(path: &str) -> bool {
    HEALTH_PATHS.contains(&path)
}

pub(crate) fn authorize(
    state: &AxumApiState,
    headers: &HeaderMap,
    required_scope: &str,
    path: &str,
    request_id: &str,
) -> Result<ApiRequestContext, ApiError> {
    let Some(resolver) = &state.tenant_resolver else {
        return Ok(ApiRequestContext {
            tenant_id: "local".to_string(),
            api_key_id: "none".to_string(),
            request_id: request_id.to_string(),
        });
    };

    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if auth_header.is_none() && is_health_path(path) {
        return Ok(ApiRequestContext {
            tenant_id: "local".to_string(),
            api_key_id: "health-bypass".to_string(),
            request_id: request_id.to_string(),
        });
    }

    let mut guard = resolver.lock().map_err(|_| {
        ApiError::with_code(
            StatusCode::INTERNAL_SERVER_ERROR,
            "auth_unavailable",
            "auth unavailable",
        )
    })?;
    let decision = guard.resolve_mut(auth_header, state.now);
    let context = auth_context_from_decision(decision, required_scope, request_id)?;
    let tenant_limit = guard.tenant_rate_limit(&context.tenant_id);
    drop(guard);

    let rate_limit = tenant_limit.or(state.default_rate_limit);
    let mut limiter = state.rate_limiter.lock().map_err(|_| {
        ApiError::with_code(
            StatusCode::INTERNAL_SERVER_ERROR,
            "rate_limiter_unavailable",
            "rate limiter unavailable",
        )
    })?;
    let rate = limiter.check(
        &context.tenant_id,
        &context.api_key_id,
        rate_limit,
        state.now,
    );
    if !rate.allowed {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded",
        ));
    }

    Ok(context)
}

pub(crate) fn require_store(state: &AxumApiState) -> Result<Arc<LocalProductStore>, ApiError> {
    state.local_store.as_ref().cloned().ok_or_else(|| {
        ApiError::with_code(
            StatusCode::SERVICE_UNAVAILABLE,
            "local_store_unavailable",
            "local store unavailable",
        )
    })
}

pub(crate) fn backup_dir_for_state(state: &AxumApiState, db_path: &Path) -> PathBuf {
    if let Some(dir) = &state.backup_dir {
        return dir.as_ref().clone();
    }
    db_path
        .parent()
        .map(|parent| parent.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

pub(crate) fn internal_error(error: String) -> ApiError {
    ApiError::with_code(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error)
}

fn auth_context_from_decision(
    decision: AuthDecision,
    required_scope: &str,
    request_id: &str,
) -> Result<ApiRequestContext, ApiError> {
    if !decision.allowed {
        return Err(ApiError::with_code(
            StatusCode::UNAUTHORIZED,
            "auth_required",
            "unauthorized",
        ));
    }
    if !decision.scopes.contains(required_scope) {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "missing_scope",
            "forbidden",
        ));
    }
    Ok(ApiRequestContext {
        tenant_id: decision.tenant_id.unwrap_or_else(|| "unknown".to_string()),
        api_key_id: decision.api_key_id.unwrap_or_else(|| "unknown".to_string()),
        request_id: request_id.to_string(),
    })
}

fn default_error_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "auth_required",
        StatusCode::FORBIDDEN => "missing_scope",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_error",
        _ => "api_error",
    }
}

pub(crate) fn cors_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let origin = cors_allowed_origin();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_str(origin).unwrap_or_else(|_| HeaderValue::from_static("*")),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,PUT,DELETE,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization,content-type"),
    );
    headers
}

fn cors_allowed_origin() -> &'static str {
    static CORS_ORIGIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CORS_ORIGIN.get_or_init(|| {
        std::env::var("ACP_CORS_ORIGINS").unwrap_or_else(|_| "*".to_string())
    })
}

pub(crate) async fn cors_preflight() -> impl IntoResponse {
    (cors_headers(), StatusCode::NO_CONTENT)
}

pub(crate) async fn request_id_layer(mut request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }
    response
}

#[derive(Debug, Clone)]
pub(crate) struct RequestId(pub String);

pub(crate) fn chrono_free_today() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono_free_date_from_unix_days(secs / 86400)
}

fn chrono_free_date_from_unix_days(mut days: u64) -> String {
    let mut y = 1970i64;
    loop {
        let leap = is_leap(y);
        let day_count = if leap { 366 } else { 365 };
        if days < day_count as u64 {
            break;
        }
        days -= day_count as u64;
        y += 1;
    }
    let mut remaining = days;
    let leap = is_leap(y);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0u32;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as u64 {
            m = i as u32 + 1;
            break;
        }
        remaining -= md as u64;
    }
    let d = remaining + 1;
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::chrono_free_date_from_unix_days;

    #[test]
    fn chrono_free_date_from_unix_days_advances_years() {
        assert_eq!(chrono_free_date_from_unix_days(0), "1970-01-01");
        assert_eq!(chrono_free_date_from_unix_days(365), "1971-01-01");
        assert_eq!(chrono_free_date_from_unix_days(365 + 365), "1972-01-01");
        assert_eq!(
            chrono_free_date_from_unix_days(365 + 365 + 31 + 28),
            "1972-02-29"
        );
    }
}
