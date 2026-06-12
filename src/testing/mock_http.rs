//! Shared mock HTTP server for adapter unit tests (RT-2).
//!
//! Consolidates the `MockHttpResponse` + `spawn_mock_http_server*` helpers that
//! were copied into five adapter test modules (facebook / instagram / reddit /
//! twitter / tikhub). This is the superset of those copies:
//!
//! * `extra_headers` + [`MockHttpResponse::with_header`] — used by the
//!   facebook/reddit rate-limit (429/`Retry-After`) tests.
//! * A union status-reason table covering every code the prior copies set.
//!
//! Observable behaviour is identical to every prior copy: the only difference is
//! the HTTP *reason phrase* for a handful of status codes, which `reqwest` never
//! exposes (it parses the numeric status). For an empty `extra_headers` the byte
//! framing (`…Connection: close\r\n\r\n<body>`) matches the old inline builders.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub(crate) struct MockHttpResponse {
    pub(crate) status: u16,
    pub(crate) body: Value,
    pub(crate) extra_headers: Vec<(String, String)>,
}

impl MockHttpResponse {
    pub(crate) fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body,
            extra_headers: Vec::new(),
        }
    }

    /// Attach an extra response header (e.g. `Retry-After`) for rate-limit tests.
    #[allow(dead_code)] // only facebook/reddit rate-limit tests use this
    pub(crate) fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }
}

/// Spawn a one-shot mock HTTP server that replays `responses` in order and
/// captures each inbound request line. Returns `(base_url, captured_requests)`.
pub(crate) async fn spawn_mock_http_server_with_capture(
    responses: Vec<MockHttpResponse>,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let expected_requests = responses.lock().unwrap().len();
    let captured_requests = requests.clone();

    tokio::spawn(async move {
        for _ in 0..expected_requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let responses = responses.clone();
            let requests = captured_requests.clone();

            tokio::spawn(async move {
                let mut buffer = vec![0_u8; 4096];
                let size = socket.read(&mut buffer).await.unwrap();
                let request_text = String::from_utf8_lossy(&buffer[..size]).to_string();
                if let Some(request_line) = request_text.lines().next() {
                    requests.lock().unwrap().push(request_line.to_string());
                }

                let response = responses.lock().unwrap().pop_front().unwrap_or_else(|| {
                    MockHttpResponse::json(500, json!({"message": "missing mock response"}))
                });

                let reason = match response.status {
                    200 => "OK",
                    400 => "Bad Request",
                    401 => "Unauthorized",
                    404 => "Not Found",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    503 => "Service Unavailable",
                    _ => "Mock Response",
                };
                let body = serde_json::to_string(&response.body).unwrap();
                let mut raw = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                    response.status,
                    reason,
                    body.len()
                );
                for (key, value) in response.extra_headers {
                    raw.push_str(&format!("{key}: {value}\r\n"));
                }
                raw.push_str("\r\n");
                raw.push_str(&body);

                socket.write_all(raw.as_bytes()).await.unwrap();
                let _ = socket.shutdown().await;
            });
        }
    });

    (format!("http://{}", addr), requests)
}

/// Convenience wrapper that discards the request capture (facebook/reddit).
#[allow(dead_code)] // only facebook/reddit use the non-capturing form
pub(crate) async fn spawn_mock_http_server(responses: Vec<MockHttpResponse>) -> String {
    let (base_url, _) = spawn_mock_http_server_with_capture(responses).await;
    base_url
}
