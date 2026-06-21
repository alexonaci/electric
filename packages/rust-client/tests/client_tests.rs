//! Wiremock-based tests for [`ShapeStream`].

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use electric_client::{
    client::{ShapeStream, ShapeStreamOptions},
    fetch::{BackoffOptions, Fetcher},
    ElectricError, Message, Offset,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer};

use support::{collect_messages, MockFetcher, MockResponse, SCHEMA_ID_TEXT};

fn no_retry_backoff() -> BackoffOptions {
    BackoffOptions {
        initial_delay_ms: 1,
        max_delay_ms: 1,
        multiplier: 1.0,
        max_retries: 0, // fail immediately on error
    }
}

// ── Basic sync ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn basic_initial_sync() {
    let server = MockServer::start().await;

    let body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"hello"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(support::shape_200("h1", "1_0", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "todos".to_string(),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    let msgs = collect_messages(stream.subscribe()).await;

    assert_eq!(msgs.len(), 2);
    assert!(matches!(msgs[0], Message::Change(_)));
    assert!(msgs[1].is_up_to_date());
}

#[tokio::test]
async fn parsed_int4_value() {
    let server = MockServer::start().await;

    let body = serde_json::json!([
        {"key":"5","value":{"id":"5","text":"world"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(support::shape_200("h1", "1_0", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "t".to_string(),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    let msgs = collect_messages(stream.subscribe()).await;

    if let Message::Change(c) = &msgs[0] {
        // int4 should be parsed to a JSON number
        assert_eq!(c.value["id"], serde_json::json!(5i64));
        // text stays as string
        assert_eq!(c.value["text"], serde_json::json!("world"));
    } else {
        panic!("Expected Change");
    }
}

// ── Shape handle tracking ──────────────────────────────────────────────────────

#[tokio::test]
async fn shape_handle_updated_after_first_response() {
    let server = MockServer::start().await;

    let body = serde_json::json!([{"headers":{"control":"up-to-date"}}]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(support::shape_200(
            "my-handle-42",
            "10_0",
            SCHEMA_ID_TEXT,
            body,
        ))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "t".to_string(),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    collect_messages(stream.subscribe()).await;

    assert_eq!(stream.shape_handle().as_deref(), Some("my-handle-42"));
}

#[tokio::test]
async fn offset_updated_after_first_response() {
    let server = MockServer::start().await;

    let body = serde_json::json!([{"headers":{"control":"up-to-date"}}]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(support::shape_200("h1", "42_7", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "t".to_string(),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    collect_messages(stream.subscribe()).await;

    assert_eq!(stream.last_offset(), Offset::At { tx: 42, op: 7 });
}

// ── 409 must-refetch ───────────────────────────────────────────────────────────

#[tokio::test]
async fn handles_409_and_retries_with_new_handle() {
    // First response: 409 with a new handle header
    let resp_409 = MockResponse {
        status: 409,
        headers: {
            let mut h = HashMap::new();
            h.insert("electric-handle".to_string(), "new-handle-99".to_string());
            h
        },
        body: serde_json::json!([
            {"headers":{"control":"must-refetch"}}
        ])
        .to_string(),
    };

    // Second response: successful 200
    let resp_200 = MockResponse::shape_200(
        "new-handle-99",
        "1_0",
        SCHEMA_ID_TEXT,
        &serde_json::json!([
            {"key":"1","value":{"id":"1","text":"re-synced"},"headers":{"operation":"insert"}},
            {"headers":{"control":"up-to-date"}}
        ])
        .to_string(),
    );

    let fetcher = MockFetcher::new(vec![resp_409, resp_200]);
    let fetcher_arc: Arc<dyn Fetcher> = Arc::new(fetcher);

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: "http://electric.test/v1/shape".to_string(),
        table: "todos".to_string(),
        subscribe: false,
        fetcher: Some(fetcher_arc),
        ..Default::default()
    })
    .unwrap();

    let msgs = collect_messages(stream.subscribe()).await;

    // Should have: must-refetch + insert + up-to-date
    assert!(msgs.iter().any(|m| m.is_must_refetch()));
    assert!(msgs.iter().any(|m| matches!(m, Message::Change(_))));
    assert!(msgs.iter().any(|m| m.is_up_to_date()));

    // Handle should be updated to the new one from the 409
    assert_eq!(stream.shape_handle().as_deref(), Some("new-handle-99"));
}

// ── Custom headers ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn custom_headers_forwarded() {
    let server = MockServer::start().await;

    let body = serde_json::json!([{"headers":{"control":"up-to-date"}}]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .and(wiremock::matchers::header("x-auth-token", "secret-123"))
        .respond_with(support::shape_200("h1", "1_0", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let mut custom_headers = HashMap::new();
    custom_headers.insert("x-auth-token".to_string(), "secret-123".to_string());

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "t".to_string(),
        subscribe: false,
        headers: custom_headers,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    collect_messages(stream.subscribe()).await;
    // If the mock matched (with header condition), the test passes
}

// ── Validation ────────────────────────────────────────────────────────────────

#[test]
fn missing_url_returns_error() {
    let result = ShapeStream::new(ShapeStreamOptions {
        url: "".to_string(),
        table: "t".to_string(),
        ..Default::default()
    });
    assert!(matches!(
        result.err().unwrap(),
        ElectricError::MissingShapeUrl
    ));
}

#[test]
fn missing_table_returns_error() {
    let result = ShapeStream::new(ShapeStreamOptions {
        url: "http://localhost:3000/v1/shape".to_string(),
        table: "".to_string(),
        ..Default::default()
    });
    assert!(matches!(
        result.err().unwrap(),
        ElectricError::MissingShapeTable
    ));
}

#[test]
fn missing_handle_with_non_initial_offset_returns_error() {
    let result = ShapeStream::new(ShapeStreamOptions {
        url: "http://localhost:3000/v1/shape".to_string(),
        table: "todos".to_string(),
        offset: Offset::At { tx: 100, op: 0 },
        handle: None,
        ..Default::default()
    });
    assert!(matches!(
        result.err().unwrap(),
        ElectricError::MissingShapeHandle
    ));
}

#[test]
fn reserved_param_names_rejected() {
    let mut params = HashMap::new();
    params.insert("offset".to_string(), "100".to_string()); // reserved!

    let result = ShapeStream::new(ShapeStreamOptions {
        url: "http://localhost:3000/v1/shape".to_string(),
        table: "t".to_string(),
        params,
        ..Default::default()
    });
    assert!(matches!(
        result.err().unwrap(),
        ElectricError::ReservedParam(_)
    ));
}

// ── is_loading / is_up_to_date ────────────────────────────────────────────────

#[tokio::test]
async fn is_loading_true_before_up_to_date() {
    let server = MockServer::start().await;

    let body = serde_json::json!([{"headers":{"control":"up-to-date"}}]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(support::shape_200("h1", "1_0", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "t".to_string(),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    // Before subscribing, stream hasn't started yet
    assert!(stream.is_loading());

    let rx = stream.subscribe();
    collect_messages(rx).await;

    assert!(stream.is_up_to_date());
    assert!(!stream.is_loading());
}

// ── WHERE clause forwarded ─────────────────────────────────────────────────────

#[tokio::test]
async fn where_clause_included_in_request() {
    let server = MockServer::start().await;

    let body = serde_json::json!([{"headers":{"control":"up-to-date"}}]);

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .and(query_param("where", "active = true"))
        .respond_with(support::shape_200("h1", "1_0", SCHEMA_ID_TEXT, body))
        .mount(&server)
        .await;

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", server.uri()),
        table: "users".to_string(),
        where_clause: Some("active = true".to_string()),
        subscribe: false,
        backoff: no_retry_backoff(),
        ..Default::default()
    })
    .unwrap();

    collect_messages(stream.subscribe()).await;
    // Mock matched with `where` param — if we get here, test passes.
}
