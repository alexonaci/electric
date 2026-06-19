//! Shared test utilities for the electric-client test suite.
//!
//! Imported as `mod support;` in each test file (Rust compiles each file
//! in `tests/` independently, so this module is compiled per-binary).

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use electric_client::{
    fetch::{ElectricRequest, ElectricResponse, Fetcher},
    ElectricError, Message,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── Wire-format helpers ───────────────────────────────────────────────────────

/// Build a complete 200 OK `ResponseTemplate` for a shape snapshot response.
pub fn shape_200(
    handle: &str,
    offset: &str,
    schema_json: &str,
    body: serde_json::Value,
) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("electric-handle", handle)
        .insert_header("electric-offset", offset)
        .insert_header("electric-schema", schema_json)
        .insert_header("electric-up-to-date", "true")
        .set_body_string(body.to_string())
}

/// Minimal schema JSON for `id` (int4) and `text` (text) columns.
pub const SCHEMA_ID_TEXT: &str =
    r#"{"id":{"type":"int4","dimensions":0},"text":{"type":"text","dimensions":0}}"#;

/// A batch containing one insert and an up-to-date control message.
pub fn single_insert_batch(key: &str, id: &str, text: &str) -> serde_json::Value {
    serde_json::json!([
        {
            "key": key,
            "value": {"id": id, "text": text},
            "headers": {"operation": "insert", "txids": [1]}
        },
        {"headers": {"control": "up-to-date"}}
    ])
}

/// Collect all messages from a broadcast receiver, stopping at `Closed` or
/// `Error`.
pub async fn collect_messages(
    mut rx: tokio::sync::broadcast::Receiver<electric_client::ShapeEvent>,
) -> Vec<Message> {
    let mut out = Vec::new();
    loop {
        match rx.recv().await {
            Ok(electric_client::ShapeEvent::Batch(batch)) => {
                out.extend((*batch).clone());
            }
            Ok(electric_client::ShapeEvent::Closed) => break,
            Ok(electric_client::ShapeEvent::Error(e)) => {
                panic!("stream error: {e}");
            }
            Err(_) => break,
        }
    }
    out
}

// ── Mock Fetcher ──────────────────────────────────────────────────────────────

/// A simple in-memory mock fetcher for unit tests that don't need a full
/// wiremock server.
///
/// Responses are consumed in order.  If exhausted, returns a 503.
#[derive(Clone)]
pub struct MockFetcher {
    responses: Arc<tokio::sync::Mutex<Vec<MockResponse>>>,
    pub requests: Arc<tokio::sync::Mutex<Vec<String>>>,
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl MockFetcher {
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(tokio::sync::Mutex::new(responses)),
            requests: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl Fetcher for MockFetcher {
    async fn fetch(&self, request: ElectricRequest) -> Result<ElectricResponse, ElectricError> {
        self.requests.lock().await.push(request.url.clone());
        let mut lock = self.responses.lock().await;
        if let Some(resp) = lock.first().cloned() {
            // Only consume retryable responses once; non-retryable return without consuming
            if resp.status >= 500 || resp.status == 429 {
                lock.remove(0);
            } else {
                lock.remove(0);
            }
            Ok(ElectricResponse {
                status: resp.status,
                headers: resp.headers,
                body: resp.body,
            })
        } else {
            Ok(ElectricResponse {
                status: 503,
                headers: HashMap::new(),
                body: "MockFetcher exhausted".to_string(),
            })
        }
    }
}

impl MockResponse {
    pub fn shape_200(handle: &str, offset: &str, schema: &str, body: &str) -> Self {
        let mut headers = HashMap::new();
        headers.insert("electric-handle".to_string(), handle.to_string());
        headers.insert("electric-offset".to_string(), offset.to_string());
        headers.insert("electric-schema".to_string(), schema.to_string());
        headers.insert("electric-up-to-date".to_string(), "true".to_string());
        Self {
            status: 200,
            headers,
            body: body.to_string(),
        }
    }

    pub fn error(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: format!("HTTP {status}"),
        }
    }
}
