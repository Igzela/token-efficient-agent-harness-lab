use std::sync::Mutex;

pub const MAX_HTTP_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const HTTP_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const HTTP_OVERALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
            .read_timeout(HTTP_READ_TIMEOUT)
            .timeout(HTTP_OVERALL_TIMEOUT)
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

        if let Some(secs) = request.timeout_secs {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(secs));
        }

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
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| HttpError::Parse(format!("failed to read body: {e}")))?
        {
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
