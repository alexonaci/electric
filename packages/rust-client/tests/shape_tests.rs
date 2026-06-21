//! Tests for [`Shape`]: materialization, insert/update/delete, must-refetch.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use electric_client::{
    client::{ShapeStream, ShapeStreamOptions},
    fetch::BackoffOptions,
    shape::Shape,
    Message,
};

use support::{MockFetcher, MockResponse, SCHEMA_ID_TEXT};

fn make_stream_with_mock(responses: Vec<MockResponse>) -> ShapeStream {
    let fetcher: Arc<dyn electric_client::fetch::Fetcher> = Arc::new(MockFetcher::new(responses));
    ShapeStream::new(ShapeStreamOptions {
        url: "http://electric.test/v1/shape".to_string(),
        table: "todos".to_string(),
        subscribe: false,
        fetcher: Some(fetcher),
        ..Default::default()
    })
    .unwrap()
}

// ── Insert ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inserts_materialize_rows() {
    let body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"Buy milk"},"headers":{"operation":"insert"}},
        {"key":"2","value":{"id":"2","text":"Write tests"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    let stream = make_stream_with_mock(vec![MockResponse::shape_200(
        "h1",
        "2_0",
        SCHEMA_ID_TEXT,
        &body.to_string(),
    )]);

    let shape = Shape::new(stream);
    let rows = shape.rows().await;

    assert_eq!(rows.len(), 2);
    let texts: Vec<&str> = rows
        .iter()
        .filter_map(|r| r.get("text").and_then(|v| v.as_str()))
        .collect();
    assert!(texts.contains(&"Buy milk"));
    assert!(texts.contains(&"Write tests"));
}

// ── Update ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_merges_changed_columns() {
    // First response: initial insert with two columns
    let insert_body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"original","done":"false"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);
    // Second response: partial update (only `text` column present)
    let update_body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"updated"},"headers":{"operation":"update"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    let fetcher = MockFetcher::new(vec![
        MockResponse::shape_200("h1", "1_0", SCHEMA_ID_TEXT, &insert_body.to_string()),
        MockResponse::shape_200("h1", "2_0", SCHEMA_ID_TEXT, &update_body.to_string()),
    ]);

    // subscribe=true so we stay in live mode and receive the second batch
    let fetcher_arc: Arc<dyn electric_client::fetch::Fetcher> = Arc::new(fetcher);
    let stream = ShapeStream::new(ShapeStreamOptions {
        url: "http://electric.test/v1/shape".to_string(),
        table: "todos".to_string(),
        subscribe: true, // stay live to get the update
        fetcher: Some(fetcher_arc),
        backoff: BackoffOptions {
            initial_delay_ms: 1,
            max_delay_ms: 1,
            multiplier: 1.0,
            max_retries: 0,
        },
        ..Default::default()
    })
    .unwrap();

    let shape = Shape::new(stream);

    // Wait for the initial snapshot to load.
    let rows_initial = shape.rows().await;
    assert_eq!(rows_initial.len(), 1);

    // Poll until the update lands (both responses are served instantly by the
    // mock, so we can't rely on observing the intermediate state).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let rows = shape.current_rows();
        if rows
            .first()
            .and_then(|r| r.get("text"))
            .and_then(|v| v.as_str())
            == Some("updated")
        {
            // `text` was updated, and the untouched `done` column must survive
            // the partial update (merge semantics, not replace).
            assert_eq!(rows[0].get("done").and_then(|v| v.as_str()), Some("false"));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for update to be applied"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

// ── Delete ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_row() {
    let body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"going away"},"headers":{"operation":"insert"}},
        {"key":"2","value":{"id":"2","text":"staying"},"headers":{"operation":"insert"}},
        {"key":"1","value":{"id":"1"},"headers":{"operation":"delete"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    let stream = make_stream_with_mock(vec![MockResponse::shape_200(
        "h1",
        "3_0",
        SCHEMA_ID_TEXT,
        &body.to_string(),
    )]);

    let shape = Shape::new(stream);
    let rows = shape.rows().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["text"], serde_json::json!("staying"));
}

// ── must-refetch clears rows ───────────────────────────────────────────────────

#[tokio::test]
async fn must_refetch_clears_existing_rows() {
    // Initial batch: insert two rows
    let insert_body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"row 1"},"headers":{"operation":"insert"}},
        {"key":"2","value":{"id":"2","text":"row 2"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    // Second batch: shape rotation — must-refetch + fresh insert
    let mut refetch_headers = HashMap::new();
    refetch_headers.insert("electric-handle".to_string(), "new-h".to_string());
    let refetch_resp = MockResponse {
        status: 409,
        headers: refetch_headers,
        body: serde_json::json!([{"headers":{"control":"must-refetch"}}]).to_string(),
    };

    let reinsert_body = serde_json::json!([
        {"key":"3","value":{"id":"3","text":"fresh row"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    let fetcher = MockFetcher::new(vec![
        MockResponse::shape_200("h1", "2_0", SCHEMA_ID_TEXT, &insert_body.to_string()),
        refetch_resp,
        MockResponse::shape_200("new-h", "1_0", SCHEMA_ID_TEXT, &reinsert_body.to_string()),
    ]);
    let fetcher_arc: Arc<dyn electric_client::fetch::Fetcher> = Arc::new(fetcher);

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: "http://electric.test/v1/shape".to_string(),
        table: "todos".to_string(),
        subscribe: true,
        fetcher: Some(fetcher_arc),
        backoff: BackoffOptions {
            initial_delay_ms: 1,
            max_delay_ms: 1,
            multiplier: 1.0,
            max_retries: 0,
        },
        ..Default::default()
    })
    .unwrap();

    let shape = Shape::new(stream);

    // Wait for initial up-to-date
    shape.rows().await;

    // Allow time for the 409 + re-sync cycle to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    let rows = shape.current_rows();
    // After re-sync, only the fresh row should remain
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["text"], serde_json::json!("fresh row"));
}

// ── Subscribe callback ─────────────────────────────────────────────────────────

#[tokio::test]
async fn subscribe_callback_called_on_up_to_date() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let body = serde_json::json!([
        {"key":"1","value":{"id":"1","text":"A"},"headers":{"operation":"insert"}},
        {"headers":{"control":"up-to-date"}}
    ]);

    let stream = make_stream_with_mock(vec![MockResponse::shape_200(
        "h1",
        "1_0",
        SCHEMA_ID_TEXT,
        &body.to_string(),
    )]);

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = call_count.clone();

    let shape = Shape::new(stream);
    let _guard = shape
        .subscribe(move |rows| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            assert_eq!(rows.len(), 1);
        })
        .await;

    // Wait for initial snapshot
    shape.rows().await;

    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

// ── current_rows before up-to-date ───────────────────────────────────────────

#[tokio::test]
async fn current_rows_empty_before_sync() {
    // Don't mount any mock — stream will error on first request
    let fetcher = MockFetcher::new(vec![]);
    let fetcher_arc: Arc<dyn electric_client::fetch::Fetcher> = Arc::new(fetcher);

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: "http://electric.test/v1/shape".to_string(),
        table: "t".to_string(),
        subscribe: false,
        fetcher: Some(fetcher_arc),
        ..Default::default()
    })
    .unwrap();

    let shape = Shape::new(stream);
    // Before the task runs, current_rows should be empty
    assert!(shape.current_rows().is_empty());
}
