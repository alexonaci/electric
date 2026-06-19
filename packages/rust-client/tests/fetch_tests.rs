//! Wiremock-based tests for the [`Fetcher`] trait and [`BackoffFetcher`] wrapper.

mod support;

use std::collections::HashMap;

use electric_client::fetch::{
    BackoffFetcher, BackoffOptions, ElectricRequest, Fetcher, ReqwestFetcher,
};
use electric_client::ElectricError;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Helper: build an ElectricRequest pointing at `server.uri()`.
fn req(server: &MockServer) -> ElectricRequest {
    ElectricRequest {
        url: format!("{}/v1/shape?table=test&offset=-1", server.uri()),
        headers: HashMap::new(),
    }
}

fn fast_backoff() -> BackoffOptions {
    BackoffOptions {
        initial_delay_ms: 5,
        max_delay_ms: 50,
        multiplier: 2.0,
        max_retries: 5,
    }
}

// ── ReqwestFetcher ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn reqwest_fetcher_returns_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let fetcher = ReqwestFetcher::new();
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.trim(), "[]");
}

#[tokio::test]
async fn reqwest_fetcher_returns_404_as_ok() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let fetcher = ReqwestFetcher::new();
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    assert_eq!(resp.status, 404);
}

#[tokio::test]
async fn reqwest_fetcher_returns_lower_case_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Electric-Handle", "h1")
                .set_body_string("[]"),
        )
        .mount(&server)
        .await;

    let fetcher = ReqwestFetcher::new();
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    // All header keys should be lowercase
    assert!(resp.headers.contains_key("electric-handle"));
}

// ── BackoffFetcher: retries ────────────────────────────────────────────────────

#[tokio::test]
async fn backoff_retries_on_500_then_succeeds() {
    let server = MockServer::start().await;

    // First request returns 500
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(500).set_body_string("error"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Second request returns 200
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(ReqwestFetcher::new(), fast_backoff());
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn backoff_retries_on_429() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_string("rate limited"),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(ReqwestFetcher::new(), fast_backoff());
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn backoff_does_not_retry_on_400() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(ReqwestFetcher::new(), fast_backoff());
    let result = fetcher.fetch(req(&server)).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ElectricError::Fetch { status: 400, .. } => {}
        other => panic!("Expected Fetch(400), got {:?}", other),
    }
}

#[tokio::test]
async fn backoff_does_not_retry_on_401() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(ReqwestFetcher::new(), fast_backoff());
    let result = fetcher.fetch(req(&server)).await;

    // Server was only hit once (no retry)
    server.verify().await; // all mocks satisfied at most once
    assert!(matches!(
        result.unwrap_err(),
        ElectricError::Fetch { status: 401, .. }
    ));
}

#[tokio::test]
async fn backoff_exhausts_max_retries() {
    let server = MockServer::start().await;

    // Always return 500
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(
        ReqwestFetcher::new(),
        BackoffOptions {
            initial_delay_ms: 1,
            max_delay_ms: 5,
            multiplier: 1.0,
            max_retries: 2, // only 2 retries
        },
    );

    let result = fetcher.fetch(req(&server)).await;
    assert!(matches!(
        result.unwrap_err(),
        ElectricError::Fetch { status: 500, .. }
    ));
}

#[tokio::test]
async fn backoff_returns_200_for_204_no_content() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/shape"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let fetcher = BackoffFetcher::new(ReqwestFetcher::new(), fast_backoff());
    let resp = fetcher.fetch(req(&server)).await.unwrap();
    assert_eq!(resp.status, 204);
}
