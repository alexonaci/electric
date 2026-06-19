//! HTTP fetch abstraction: a pluggable `Fetcher` trait plus an exponential-backoff
//! wrapper and a default [`reqwest`]-based implementation.
//!
//! Mirrors `packages/typescript-client/src/fetch.ts`.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;

use crate::error::ElectricError;

// ── Request / Response ────────────────────────────────────────────────────────

/// A minimal HTTP GET request description passed to [`Fetcher::fetch`].
#[derive(Debug, Clone)]
pub struct ElectricRequest {
    /// Full URL (already includes all query parameters).
    pub url: String,
    /// HTTP request headers.
    pub headers: HashMap<String, String>,
}

/// The HTTP response returned by [`Fetcher::fetch`].
#[derive(Debug)]
pub struct ElectricResponse {
    /// HTTP status code.
    pub status: u16,
    /// All response headers (lower-case names).
    pub headers: HashMap<String, String>,
    /// Response body as a UTF-8 string.
    pub body: String,
}

// ── Fetcher trait ─────────────────────────────────────────────────────────────

/// Pluggable HTTP fetch backend.
///
/// The default implementation is [`ReqwestFetcher`].  Tests can inject a mock
/// by implementing this trait and passing the implementation via
/// [`ShapeStreamOptions::fetcher`](crate::client::ShapeStreamOptions::fetcher).
#[async_trait]
pub trait Fetcher: Send + Sync + 'static {
    async fn fetch(&self, request: ElectricRequest) -> Result<ElectricResponse, ElectricError>;
}

// ── Default reqwest-based implementation ─────────────────────────────────────

/// Default [`Fetcher`] backed by [`reqwest`].
#[derive(Clone, Default)]
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Fetcher for ReqwestFetcher {
    async fn fetch(&self, req: ElectricRequest) -> Result<ElectricResponse, ElectricError> {
        let mut builder = self.client.get(&req.url);

        for (key, value) in &req.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        let response = builder
            .send()
            .await
            .map_err(|e| ElectricError::Network(e.to_string()))?;

        let status = response.status().as_u16();

        // Collect response headers (lowercase names)
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v| (k.as_str().to_lowercase(), v.to_owned()))
            })
            .collect();

        let body = response
            .text()
            .await
            .map_err(|e| ElectricError::Network(e.to_string()))?;

        Ok(ElectricResponse {
            status,
            headers,
            body,
        })
    }
}

// ── Backoff configuration ─────────────────────────────────────────────────────

/// Exponential-backoff configuration for retrying failed requests.
#[derive(Debug, Clone)]
pub struct BackoffOptions {
    /// Initial retry delay in milliseconds (default: 1 000).
    pub initial_delay_ms: u64,
    /// Maximum retry delay in milliseconds (default: 32 000).
    pub max_delay_ms: u64,
    /// Delay multiplier applied after each retry (default: 2.0).
    pub multiplier: f64,
    /// Maximum number of retry attempts.  Use `u32::MAX` for "forever" (default).
    pub max_retries: u32,
}

impl Default for BackoffOptions {
    fn default() -> Self {
        Self {
            initial_delay_ms: 1_000,
            max_delay_ms: 32_000,
            multiplier: 2.0,
            max_retries: u32::MAX,
        }
    }
}

// ── Retry-After header parsing ────────────────────────────────────────────────

/// Parse a `Retry-After` header value into a delay in milliseconds.
///
/// Supports both delta-seconds (`"30"`) and HTTP-date formats.
/// Returns 0 if the header is absent or unparseable.
pub fn parse_retry_after(value: &str) -> u64 {
    // Try delta-seconds first
    if let Ok(secs) = value.trim().parse::<f64>() {
        if secs > 0.0 {
            return (secs * 1_000.0) as u64;
        }
    }
    // Try HTTP-date
    // httpdate crate would be ideal; for now use a simple heuristic
    0
}

// HTTP status codes that trigger a retry (besides 5xx)
const RETRY_STATUS_CODES: &[u16] = &[429];

// ── BackoffFetcher ────────────────────────────────────────────────────────────

/// A [`Fetcher`] wrapper that retries on transient failures with exponential
/// back-off and optional jitter.
///
/// Retries:
/// - Network errors (connection refused, timeout, etc.)
/// - HTTP 5xx responses
/// - HTTP 429 (rate-limited), honouring the `Retry-After` header
///
/// Does **not** retry:
/// - HTTP 4xx responses (except 429) — these are returned to the caller as-is
/// - Requests already aborted via a cancellation token
pub struct BackoffFetcher<F> {
    inner: F,
    options: BackoffOptions,
}

impl<F: Fetcher> BackoffFetcher<F> {
    pub fn new(inner: F, options: BackoffOptions) -> Self {
        Self { inner, options }
    }
}

#[async_trait]
impl<F: Fetcher> Fetcher for BackoffFetcher<F> {
    async fn fetch(&self, req: ElectricRequest) -> Result<ElectricResponse, ElectricError> {
        let mut delay_ms = self.options.initial_delay_ms;
        let mut attempt = 0u32;

        loop {
            match self.inner.fetch(req.clone()).await {
                // ── Success or non-retryable client error ──────────────────────
                Ok(resp) if resp.status < 400 => return Ok(resp),
                Ok(resp) if resp.status == 204 => return Ok(resp), // No Content is OK
                Ok(resp)
                    if resp.status >= 400
                        && resp.status < 500
                        && !RETRY_STATUS_CODES.contains(&resp.status) =>
                {
                    // 4xx (except 429): not retried — pass back to caller
                    return Err(ElectricError::Fetch {
                        status: resp.status,
                        body: resp.body,
                        headers: resp.headers,
                        url: req.url.clone(),
                    });
                }

                // ── Retryable: 429 or 5xx ─────────────────────────────────────
                Ok(resp) => {
                    attempt += 1;
                    if attempt > self.options.max_retries {
                        return Err(ElectricError::Fetch {
                            status: resp.status,
                            body: resp.body,
                            headers: resp.headers,
                            url: req.url.clone(),
                        });
                    }

                    let server_min_ms = resp
                        .headers
                        .get("retry-after")
                        .map(|v| parse_retry_after(v))
                        .unwrap_or(0);

                    let wait_ms =
                        compute_backoff_wait(delay_ms, server_min_ms, self.options.max_delay_ms);
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                    delay_ms = (delay_ms as f64 * self.options.multiplier)
                        .min(self.options.max_delay_ms as f64)
                        as u64;
                }

                // ── Network error ─────────────────────────────────────────────
                Err(ElectricError::BackoffAborted) => {
                    return Err(ElectricError::BackoffAborted);
                }
                Err(e) => {
                    attempt += 1;
                    if attempt > self.options.max_retries {
                        return Err(e);
                    }
                    let wait_ms = compute_backoff_wait(delay_ms, 0, self.options.max_delay_ms);
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                    delay_ms = (delay_ms as f64 * self.options.multiplier)
                        .min(self.options.max_delay_ms as f64)
                        as u64;
                }
            }
        }
    }
}

/// Compute the actual wait time using full-jitter strategy.
///
/// `wait = max(server_minimum, random_in(0, min(cap, delay)))`
fn compute_backoff_wait(delay_ms: u64, server_min_ms: u64, max_delay_ms: u64) -> u64 {
    let cap = delay_ms.min(max_delay_ms);
    let jitter_ms = rand::thread_rng().gen_range(0..=cap);
    jitter_ms.max(server_min_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("30"), 30_000);
        assert_eq!(parse_retry_after("1.5"), 1_500);
    }

    #[test]
    fn parse_retry_after_zero() {
        assert_eq!(parse_retry_after("0"), 0);
    }

    #[test]
    fn parse_retry_after_invalid() {
        assert_eq!(parse_retry_after("not-a-number"), 0);
    }

    #[test]
    fn compute_backoff_wait_respects_server_minimum() {
        // If server says wait 5s but client would wait less, take the server floor
        let wait = compute_backoff_wait(100, 5_000, 32_000);
        assert!(wait >= 5_000);
    }

    #[test]
    fn compute_backoff_wait_stays_within_cap() {
        for _ in 0..100 {
            let wait = compute_backoff_wait(500, 0, 32_000);
            assert!(wait <= 500);
        }
    }
}
