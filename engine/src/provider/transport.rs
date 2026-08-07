use std::sync::Mutex;

pub const MAX_HTTP_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Bounded connection-establishment timeout. Connect is transport-level and
/// always bounded; it is not a response-arrival ceiling.
const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Default total request timeout used only when the caller authorizes no
/// per-request timeout. It never overrides an authorized timeout: the
/// per-request budget from `HttpRequest.timeout_secs` is the sole authority
/// whenever it is present and finite.
const HTTP_DEFAULT_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Absolute transport safety ceiling: no single external request may exceed
/// this total wall time. It exists only to keep every request finite against
/// absurd or non-finite caller values, and it never truncates an admitted RWE
/// live envelope (the RWE admission layer bounds authorized timeouts to
/// 900 s), so it is a pure safety net, not a second timeout authority.
const HTTP_ABSOLUTE_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3600);

/// Resolves the effective total timeout for one request.
///
/// The authorized per-request budget wins whenever it is present and finite
/// (`HttpRequest.timeout_secs`); otherwise the default applies. Either way the
/// absolute safety ceiling bounds the result, so no request can ever wait
/// unboundedly and no authorized budget is silently truncated below the
/// admitted envelope.
fn effective_total_timeout(request: &HttpRequest) -> std::time::Duration {
    let authorized = request
        .timeout_secs
        .filter(|secs| secs.is_finite() && *secs > 0.0)
        .map(std::time::Duration::from_secs_f64)
        .unwrap_or(HTTP_DEFAULT_TOTAL_TIMEOUT);
    authorized.min(HTTP_ABSOLUTE_TOTAL_TIMEOUT)
}

/// Classifies a body-read failure. A transport expiry while reading the body
/// is a timeout, not a malformed response; anything else that ends the body
/// early (truncation, reset, malformed framing) is a `Parse` — the response
/// did not deliver its declared content. This matters for managed callers:
/// `Timeout` maps to `provider_timeout`/`OutcomeUnknown` (no automatic retry)
/// instead of a misleading `provider_response: transport malformed`. Connect-
/// phase failures are classified before the body exists (see `send`), so the
/// body-phase branch needs no connection case.
fn classify_body_read_error(error: reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout(error.to_string())
    } else {
        HttpError::Parse(format!("failed to read body: {error}"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_secs: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HttpError {
    PreSend(String),
    Connection(String),
    Timeout(String),
    Http { status: u16, reason: String },
    Parse(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::PreSend(msg) => write!(f, "pre-send error: {msg}"),
            HttpError::Connection(msg) => write!(f, "connection error: {msg}"),
            HttpError::Timeout(msg) => write!(f, "timeout: {msg}"),
            HttpError::Http { status, reason } => write!(f, "HTTP {status}: {reason}"),
            HttpError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

/// Provenance of the HTTP transport that serves managed provider requests.
///
/// It is a property of the transport object actually executing the request, not
/// a caller-supplied flag: the production `ReqwestTransport` declares
/// `External`; `MockTransport` and other test-injected transports declare
/// `Injected`. The durable provider journal records this per request, and a
/// live baseline seal is impossible while any claim is not `external`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTransportProvenance {
    External,
    Injected,
}

impl ProviderTransportProvenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderTransportProvenance::External => "external",
            ProviderTransportProvenance::Injected => "injected",
        }
    }
}

#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync + std::any::Any {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Canonical provenance minting for a serving transport object.
///
/// `External` is minted only when the concrete transport is the production
/// `ReqwestTransport` boundary. Transport objects cannot self-declare
/// provenance — the trait has no provenance method — so a custom or injected
/// transport that mimics the interface can never mint `External`. Anything
/// else (mock transports, wrapper boundaries, any future implementation)
/// fails closed to `Injected`. The production authority is therefore the
/// concrete canonical type, checked by the provider owner, not a flag any
/// implementor can set.
pub fn production_transport_provenance(
    transport: &std::sync::Arc<dyn HttpTransport>,
) -> ProviderTransportProvenance {
    let any = std::sync::Arc::clone(transport) as std::sync::Arc<dyn std::any::Any + Send + Sync>;
    if any.downcast::<ReqwestTransport>().is_ok() {
        ProviderTransportProvenance::External
    } else {
        ProviderTransportProvenance::Injected
    }
}

/// Injectable seam boundary. The coordinator wraps any test-injected transport
/// in this boundary before it reaches the provider, so the fake seam can never
/// be the canonical `ReqwestTransport` concrete type regardless of what the
/// caller passed in: every request served through a fake seam is `Injected`
/// by construction, and no fake transport can upgrade itself to a live
/// external baseline.
pub struct InjectedTransportBoundary(pub std::sync::Arc<dyn HttpTransport>);

#[async_trait::async_trait]
impl HttpTransport for InjectedTransportBoundary {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.0.send(request).await
    }
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self { client }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = request
            .method
            .parse::<reqwest::Method>()
            .map_err(|e| HttpError::Parse(format!("invalid method: {e}")))?;

        let url: reqwest::Url = request
            .url
            .parse()
            .map_err(|e| HttpError::Parse(format!("invalid url: {e}")))?;

        let mut req_builder = self.client.request(method, url);

        for (key, value) in &request.headers {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }

        if let Some(body) = &request.body {
            req_builder = req_builder.body(body.clone());
        }

        let deadline = tokio::time::Instant::now() + effective_total_timeout(request);
        req_builder = req_builder.timeout(effective_total_timeout(request));

        let mut response = req_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout(e.to_string())
            } else {
                HttpError::Connection(e.to_string())
            }
        })?;

        let status = response.status().as_u16();
        if (300..=399).contains(&status) {
            return Err(HttpError::Http {
                status,
                reason: "redirect refused".to_string(),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_HTTP_RESPONSE_BYTES as u64)
        {
            return Err(HttpError::Parse("response body limit exceeded".to_string()));
        }
        let mut body = Vec::new();
        // reqwest's per-request timeout ends when response headers arrive, so
        // the authorized total must also bound the body read itself. The
        // absolute deadline computed once at request start keeps the whole
        // request inside the authorized budget and fails closed with a timeout
        // even if the server stalls mid-body.
        loop {
            let chunk = match tokio::time::timeout_at(deadline, response.chunk()).await {
                Err(_) => {
                    return Err(HttpError::Timeout(
                        "response body read exceeded authorized total timeout".to_string(),
                    ))
                }
                Ok(chunk_result) => chunk_result.map_err(classify_body_read_error)?,
            };
            let Some(chunk) = chunk else { break };
            if body.len().saturating_add(chunk.len()) > MAX_HTTP_RESPONSE_BYTES {
                return Err(HttpError::Parse("response body limit exceeded".to_string()));
            }
            body.extend_from_slice(&chunk);
        }

        if status >= 400 {
            let reason = String::from_utf8_lossy(&body).to_string();
            return Err(HttpError::Http {
                status,
                reason: if reason.is_empty() {
                    format!("HTTP {status}")
                } else {
                    reason
                },
            });
        }

        Ok(HttpResponse { status, body })
    }
}

pub struct MockTransport {
    responses: Mutex<Vec<Result<HttpResponse, HttpError>>>,
}

impl MockTransport {
    pub fn new(responses: Vec<Result<HttpResponse, HttpError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn push(&self, response: Result<HttpResponse, HttpError>) {
        self.responses.lock().unwrap().push(response);
    }
}

#[async_trait::async_trait]
impl HttpTransport for MockTransport {
    async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        if responses.is_empty() {
            Err(HttpError::Connection(
                "no mock responses available".to_string(),
            ))
        } else {
            responses.remove(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const TEST_RESPONSE_LIMIT: usize = 2 * 1024 * 1024;

    fn serve_once(response: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = [0_u8; 4_096];
            let _ = stream.read(&mut request);
            stream.write_all(&response).unwrap();
        });
        (format!("http://{address}"), handle)
    }

    /// Serves one response after the given delay has elapsed since the request
    /// arrived, to prove the client honors an authorized total timeout rather
    /// than a fixed arrival/read cap.
    fn serve_once_after(
        delay: std::time::Duration,
        response: Vec<u8>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4_096];
            let _ = stream.read(&mut request);
            std::thread::sleep(delay);
            stream.write_all(&response).unwrap();
        });
        (format!("http://{address}"), handle)
    }

    /// Serves headers and a partial body immediately, then stalls, to prove
    /// that a mid-body stall is governed by the authorized total timeout and
    /// classified as a timeout, not a malformed response.
    fn serve_headers_then_stall(
        headers: Vec<u8>,
        stall: std::time::Duration,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4_096];
            let _ = stream.read(&mut request);
            stream.write_all(&headers).unwrap();
            stream.flush().unwrap();
            std::thread::sleep(stall);
        });
        (format!("http://{address}"), handle)
    }

    fn request(url: String) -> HttpRequest {
        HttpRequest {
            url,
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: Some(2.0),
        }
    }

    #[tokio::test]
    async fn mock_transport_pops_front() {
        let transport = MockTransport::new(vec![
            Ok(HttpResponse {
                status: 200,
                body: b"first".to_vec(),
            }),
            Ok(HttpResponse {
                status: 201,
                body: b"second".to_vec(),
            }),
        ]);
        let req = HttpRequest {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: None,
        };
        let r1 = transport.send(&req).await.unwrap();
        assert_eq!(r1.status, 200);
        assert_eq!(r1.body, b"first");
        let r2 = transport.send(&req).await.unwrap();
        assert_eq!(r2.status, 201);
        assert_eq!(r2.body, b"second");
        assert!(transport.send(&req).await.is_err());
    }

    #[tokio::test]
    async fn mock_transport_error() {
        let transport = MockTransport::new(vec![Err(HttpError::Timeout("timed out".to_string()))]);
        let req = HttpRequest {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: None,
        };
        let result = transport.send(&req).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HttpError::Timeout(_) => {}
            other => panic!("expected Timeout, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mock_transport_empty() {
        let transport = MockTransport::empty();
        let req = HttpRequest {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: None,
        };
        assert!(transport.send(&req).await.is_err());
    }

    #[tokio::test]
    async fn reqwest_transport_rejects_redirect_without_following_it() {
        let (url, server) = serve_once(
            b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        );
        let error = ReqwestTransport::new()
            .send(&request(url))
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(matches!(error, HttpError::Http { status: 302, .. }));
    }

    #[tokio::test]
    async fn reqwest_transport_rejects_oversized_fixed_length_body() {
        let body = vec![b'x'; TEST_RESPONSE_LIMIT + 1];
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(&body);
        let (url, server) = serve_once(response);
        let error = ReqwestTransport::new()
            .send(&request(url))
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(
            matches!(error, HttpError::Parse(message) if message.contains("response body limit"))
        );
    }

    #[tokio::test]
    async fn reqwest_transport_rejects_oversized_chunked_body() {
        let first = vec![b'a'; TEST_RESPONSE_LIMIT];
        let second = *b"b";
        let mut response =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        response.extend_from_slice(format!("{:x}\r\n", first.len()).as_bytes());
        response.extend_from_slice(&first);
        response.extend_from_slice(b"\r\n1\r\n");
        response.extend_from_slice(&second);
        response.extend_from_slice(b"\r\n0\r\n\r\n");
        let (url, server) = serve_once(response);
        let error = ReqwestTransport::new()
            .send(&request(url))
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(
            matches!(error, HttpError::Parse(message) if message.contains("response body limit"))
        );
    }

    #[tokio::test]
    async fn reqwest_transport_rejects_oversized_decoded_gzip_body() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder
            .write_all(&vec![b'z'; TEST_RESPONSE_LIMIT + 1])
            .unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < TEST_RESPONSE_LIMIT);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            compressed.len()
        )
        .into_bytes();
        response.extend_from_slice(&compressed);
        let (url, server) = serve_once(response);
        let error = ReqwestTransport::new()
            .send(&request(url))
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(
            matches!(error, HttpError::Parse(message) if message.contains("response body limit"))
        );
    }

    #[tokio::test]
    async fn reqwest_transport_rejects_truncated_body() {
        let (url, server) = serve_once(
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".to_vec(),
        );
        let error = ReqwestTransport::new()
            .send(&request(url))
            .await
            .unwrap_err();
        server.join().unwrap();
        assert!(
            matches!(error, HttpError::Parse(message) if message.contains("failed to read body"))
        );
    }

    #[test]
    fn effective_total_timeout_respects_authorized_budget_and_ceiling() {
        let req = |secs: Option<f64>| HttpRequest {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: secs,
        };
        let s = std::time::Duration::from_secs;
        // An admitted managed envelope (up to 900 s) is never truncated by the
        // transport: the authorized budget is the sole timeout authority and
        // there is no hidden 20 s/30 s arrival cap.
        assert_eq!(effective_total_timeout(&req(Some(900.0))), s(900));
        assert_eq!(effective_total_timeout(&req(Some(21.0))), s(21));
        assert_eq!(
            effective_total_timeout(&req(Some(0.5))),
            s(0) + std::time::Duration::from_millis(500)
        );
        // No authorized budget means the finite default applies.
        assert_eq!(effective_total_timeout(&req(None)), s(30));
        // Non-finite or non-positive values fail closed to the default rather
        // than panicking or waiting unboundedly.
        assert_eq!(effective_total_timeout(&req(Some(f64::NAN))), s(30));
        assert_eq!(effective_total_timeout(&req(Some(f64::INFINITY))), s(30));
        assert_eq!(effective_total_timeout(&req(Some(-5.0))), s(30));
        assert_eq!(effective_total_timeout(&req(Some(0.0))), s(30));
        // The absolute safety ceiling keeps even absurd values finite.
        assert_eq!(effective_total_timeout(&req(Some(1e9))), s(3600));
    }

    /// Regression proof for the removed hidden 20 s read cap: a response whose
    /// arrival takes longer than the legacy cap but within the authorized
    /// budget must succeed. With the old client-level read timeout this test
    /// failed with a transport error at ~20 s.
    #[tokio::test]
    async fn authorized_timeout_longer_than_legacy_read_cap_succeeds() {
        let (url, server) = serve_once_after(
            std::time::Duration::from_secs(21),
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec(),
        );
        let mut req = request(url);
        req.timeout_secs = Some(25.0);
        let start = std::time::Instant::now();
        let result = ReqwestTransport::new().send(&req).await;
        server.join().unwrap();
        let elapsed = start.elapsed();
        assert_eq!(result.unwrap().body, b"ok");
        assert!(
            elapsed < std::time::Duration::from_secs(24),
            "response arrival took {elapsed:?}; authorized 25 s budget must govern"
        );
    }

    /// A mid-body stall is governed by the authorized total timeout and must
    /// surface as a timeout (provider_timeout/OutcomeUnknown downstream), not
    /// as a malformed-response parse error.
    #[tokio::test]
    async fn body_stall_exceeds_authorized_timeout_is_classified_as_timeout() {
        let headers =
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial".to_vec();
        let (url, server) = serve_headers_then_stall(headers, std::time::Duration::from_secs(10));
        let mut req = request(url);
        req.timeout_secs = Some(3.0);
        let start = std::time::Instant::now();
        let error = ReqwestTransport::new().send(&req).await.unwrap_err();
        let elapsed = start.elapsed();
        server.join().unwrap();
        assert!(
            matches!(error, HttpError::Timeout(_)),
            "expected Timeout, got {error:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(6),
            "total timeout must fail closed within the authorized budget, took {elapsed:?}, error: {error:?}"
        );
    }

    #[tokio::test]
    async fn connect_refused_is_classified_as_connection() {
        // System-proxy discovery is disabled so an environment forward proxy
        // cannot answer the refused loopback port on our behalf.
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let transport = ReqwestTransport::with_client(client);
        let req = HttpRequest {
            url: "http://127.0.0.1:1/".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: Some(2.0),
        };
        let error = transport.send(&req).await.unwrap_err();
        assert!(
            matches!(error, HttpError::Connection(_)),
            "expected Connection, got {error:?}"
        );
    }

    /// The connect phase stays bounded even when no response ever arrives: a
    /// dropped-SYN destination must yield a timeout/connection error within a
    /// short window, never a parse error or an unbounded wait. The test client
    /// disables system-proxy discovery so an environment forward proxy cannot
    /// answer the blackhole address on our behalf.
    #[tokio::test]
    async fn connect_timeout_stays_bounded() {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap();
        let transport = ReqwestTransport::with_client(client);
        let req = HttpRequest {
            url: "http://10.255.255.1:80/".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: Some(60.0),
        };
        let start = std::time::Instant::now();
        let error = transport.send(&req).await.unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            matches!(error, HttpError::Connection(_) | HttpError::Timeout(_)),
            "expected a bounded Connection/Timeout error, got {error:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "connect must stay bounded, took {elapsed:?}"
        );
    }

    #[test]
    fn http_error_display() {
        assert_eq!(
            HttpError::Connection("refused".to_string()).to_string(),
            "connection error: refused"
        );
        assert_eq!(
            HttpError::Timeout("30s".to_string()).to_string(),
            "timeout: 30s"
        );
        assert_eq!(
            HttpError::Http {
                status: 404,
                reason: "not found".to_string()
            }
            .to_string(),
            "HTTP 404: not found"
        );
        assert_eq!(
            HttpError::Parse("bad json".to_string()).to_string(),
            "parse error: bad json"
        );
    }

    #[test]
    fn http_request_struct() {
        let req = HttpRequest {
            url: "http://example.com".to_string(),
            method: "POST".to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: Some(b"{}".to_vec()),
            timeout_secs: Some(30.0),
        };
        assert_eq!(req.url, "http://example.com");
        assert_eq!(req.method, "POST");
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.body, Some(b"{}".to_vec()));
    }

    #[test]
    fn http_response_struct() {
        let resp = HttpResponse {
            status: 200,
            body: b"ok".to_vec(),
        };
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"ok");
    }

    #[test]
    fn mock_transport_push() {
        let transport = MockTransport::empty();
        transport.push(Ok(HttpResponse {
            status: 200,
            body: b"ok".to_vec(),
        }));
        let req = HttpRequest {
            url: "http://example.com".to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout_secs: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(transport.send(&req)).unwrap();
        assert_eq!(result.status, 200);
    }

    /// A custom transport implementation has no provenance surface at all:
    /// the trait offers no self-declaration method, so it cannot impersonate
    /// the production boundary. Only the concrete ReqwestTransport type mints
    /// External; the injectable seam boundary stays Injected even when it
    /// wraps a real ReqwestTransport (fail-closed by construction).
    struct SpoofTransport;

    #[async_trait::async_trait]
    impl HttpTransport for SpoofTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            Ok(HttpResponse {
                status: 200,
                body: b"{}".to_vec(),
            })
        }
    }

    #[test]
    fn production_provenance_mints_external_only_for_reqwest_concrete_type() {
        let reqwest: std::sync::Arc<dyn HttpTransport> =
            std::sync::Arc::new(ReqwestTransport::new());
        assert_eq!(
            production_transport_provenance(&reqwest),
            ProviderTransportProvenance::External,
            "the canonical production boundary is the only External source"
        );
        let mock: std::sync::Arc<dyn HttpTransport> =
            std::sync::Arc::new(MockTransport::new(vec![]));
        assert_eq!(
            production_transport_provenance(&mock),
            ProviderTransportProvenance::Injected
        );
        let spoof: std::sync::Arc<dyn HttpTransport> = std::sync::Arc::new(SpoofTransport);
        assert_eq!(
            production_transport_provenance(&spoof),
            ProviderTransportProvenance::Injected,
            "a custom transport cannot self-declare External"
        );
        // Strongest spoof attempt: smuggle the real production boundary through
        // the injectable seam. The seam wrapper is not the canonical concrete
        // type, so the minted provenance is Injected.
        let smuggled: std::sync::Arc<dyn HttpTransport> = std::sync::Arc::new(
            InjectedTransportBoundary(std::sync::Arc::new(ReqwestTransport::new())),
        );
        assert_eq!(
            production_transport_provenance(&smuggled),
            ProviderTransportProvenance::Injected,
            "the fake seam stays Injected even when it wraps ReqwestTransport"
        );
    }
}
