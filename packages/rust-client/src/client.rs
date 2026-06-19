//! `ShapeStream`: the core polling client for the Electric sync protocol.
//!
//! Manages the HTTP long-poll loop, tracks offset/handle/schema state, handles
//! 409 shape rotations, and fans out message batches to subscribers.
//!
//! Mirrors `packages/typescript-client/src/client.ts`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::constants::{
    CACHE_BUSTER_QUERY_PARAM, CHUNK_LAST_OFFSET_HEADER, CHUNK_UP_TO_DATE_HEADER,
    COLUMNS_QUERY_PARAM, LIVE_CACHE_BUSTER_HEADER, LIVE_CACHE_BUSTER_QUERY_PARAM, LIVE_QUERY_PARAM,
    OFFSET_QUERY_PARAM, REPLICA_PARAM, RESERVED_PARAMS, SHAPE_HANDLE_HEADER,
    SHAPE_HANDLE_QUERY_PARAM, SHAPE_SCHEMA_HEADER, TABLE_QUERY_PARAM, WHERE_PARAMS_PARAM,
    WHERE_QUERY_PARAM,
};
use crate::error::ElectricError;
use crate::fetch::{BackoffFetcher, BackoffOptions, ElectricRequest, Fetcher, ReqwestFetcher};
use crate::parser::Parser;
use crate::types::{Message, Offset, Replica, Schema};

// ── ShapeStreamOptions ────────────────────────────────────────────────────────

/// Configuration for a [`ShapeStream`].
///
/// Only `url` and `table` are required; all other fields use sensible defaults.
///
/// ```rust,no_run
/// use electric_client::{ShapeStream, ShapeStreamOptions};
///
/// let stream = ShapeStream::new(ShapeStreamOptions {
///     url: "http://localhost:3000/v1/shape".to_string(),
///     table: "todos".to_string(),
///     ..Default::default()
/// }).unwrap();
/// ```
#[derive(Clone)]
pub struct ShapeStreamOptions {
    /// Electric endpoint URL (or proxy). **Required.**
    pub url: String,
    /// Root Postgres table to subscribe to. **Required.**
    pub table: String,
    /// Starting offset.  Defaults to `Offset::Initial` (`"-1"`).
    pub offset: Offset,
    /// Shape handle for resuming a previous stream (required when `offset != Initial`).
    pub handle: Option<String>,
    /// Optional SQL WHERE clause for server-side row filtering.
    pub where_clause: Option<String>,
    /// Optional positional params for the WHERE clause (keys `"1"`, `"2"`, …).
    pub params: HashMap<String, String>,
    /// Column subset to sync (must include primary key columns).
    pub columns: Option<Vec<String>>,
    /// Replica mode: how much data is included in update/delete messages.
    pub replica: Replica,
    /// Extra HTTP headers forwarded with every request (e.g. auth tokens).
    pub headers: HashMap<String, String>,
    /// Back-off configuration for retrying transient failures.
    pub backoff: BackoffOptions,
    /// Whether to continue long-polling after the initial snapshot is delivered
    /// (`true`, default) or stop after the first `up-to-date` message (`false`).
    pub subscribe: bool,
    /// Postgres value parser.  Custom entries are merged with built-in defaults.
    pub parser: Parser,
    /// Pluggable HTTP backend.  Defaults to `ReqwestFetcher`.
    /// Useful for injecting mock implementations in tests.
    pub fetcher: Option<Arc<dyn Fetcher>>,
}

impl Default for ShapeStreamOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            table: String::new(),
            offset: Offset::Initial,
            handle: None,
            where_clause: None,
            params: HashMap::new(),
            columns: None,
            replica: Replica::Default,
            headers: HashMap::new(),
            backoff: BackoffOptions::default(),
            subscribe: true,
            parser: Parser::new(),
            fetcher: None,
        }
    }
}

// ── ShapeEvent ────────────────────────────────────────────────────────────────

/// Events emitted on the broadcast channel by a running [`ShapeStream`].
#[derive(Debug, Clone)]
pub enum ShapeEvent {
    /// A batch of messages, ending with (or consisting only of) control events.
    ///
    /// Each batch corresponds to one successful HTTP response from Electric.
    Batch(Arc<Vec<Message>>),
    /// A fatal error occurred; the stream has stopped.
    Error(String),
    /// The stream has finished cleanly (non-`subscribe` mode reached up-to-date).
    Closed,
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct StreamState {
    offset: Offset,
    handle: Option<String>,
    schema: Schema,
    cursor: Option<String>,
    is_loading: bool,
    is_up_to_date: bool,
    live: bool,
    last_synced_at: Option<u64>,
}

impl StreamState {
    fn initial(offset: Offset, handle: Option<String>) -> Self {
        Self {
            offset,
            handle,
            schema: Schema::new(),
            cursor: None,
            is_loading: true,
            is_up_to_date: false,
            live: false,
            last_synced_at: None,
        }
    }
}

struct ShapeStreamInner {
    options: ShapeStreamOptions,
    state: RwLock<StreamState>,
    sender: broadcast::Sender<ShapeEvent>,
    started: AtomicBool,
    cancel: CancellationToken,
    // Stores the error if the stream terminates with one
    last_error: Mutex<Option<String>>,
}

// ── ShapeStream ───────────────────────────────────────────────────────────────

/// Long-poll client for a single Electric shape.
///
/// `ShapeStream` manages the HTTP polling loop, tracks offset / shape-handle /
/// schema state across requests, handles 409 shape-rotation, and fans out
/// message batches to all registered subscribers.
///
/// ## Usage
///
/// ```rust,no_run
/// use electric_client::{ShapeStream, ShapeStreamOptions, ShapeEvent};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let stream = ShapeStream::new(ShapeStreamOptions {
///         url: "http://localhost:3000/v1/shape".to_string(),
///         table: "todos".to_string(),
///         subscribe: false,
///         ..Default::default()
///     }).unwrap();
///
///     let mut rx = stream.subscribe();
///     while let Ok(event) = rx.recv().await {
///         match event {
///             ShapeEvent::Batch(msgs) => println!("got {} messages", msgs.len()),
///             ShapeEvent::Closed => break,
///             ShapeEvent::Error(e) => eprintln!("error: {e}"),
///         }
///     }
/// }
/// ```
pub struct ShapeStream {
    inner: Arc<ShapeStreamInner>,
}

impl ShapeStream {
    /// Create a new `ShapeStream`.  Validates options but does **not** yet
    /// start polling.  Polling begins when the first subscriber is registered
    /// via [`subscribe`](Self::subscribe).
    pub fn new(options: ShapeStreamOptions) -> Result<Self, ElectricError> {
        // Validate required fields
        if options.url.is_empty() {
            return Err(ElectricError::MissingShapeUrl);
        }
        if options.table.is_empty() {
            return Err(ElectricError::MissingShapeTable);
        }
        if !matches!(options.offset, Offset::Initial) && options.handle.is_none() {
            return Err(ElectricError::MissingShapeHandle);
        }
        // Check for reserved param names in custom params
        let reserved: Vec<String> = options
            .params
            .keys()
            .filter(|k| RESERVED_PARAMS.contains(&k.as_str()))
            .cloned()
            .collect();
        if !reserved.is_empty() {
            return Err(ElectricError::ReservedParam(reserved));
        }

        let (sender, _) = broadcast::channel(256);
        let inner = Arc::new(ShapeStreamInner {
            state: RwLock::new(StreamState::initial(
                options.offset.clone(),
                options.handle.clone(),
            )),
            options,
            sender,
            started: AtomicBool::new(false),
            cancel: CancellationToken::new(),
            last_error: Mutex::new(None),
        });

        Ok(Self { inner })
    }

    // ── Subscription / streaming ──────────────────────────────────────────────

    /// Subscribe to events from this stream.
    ///
    /// Returns a `broadcast::Receiver` that will receive [`ShapeEvent`]s.
    /// The receiver **must** be obtained before the background task can
    /// progress past the first batch (the channel has capacity 256, so
    /// a short window exists for calling `subscribe()` after creation).
    ///
    /// This call also starts the background polling task if it has not started
    /// yet.
    pub fn subscribe(&self) -> broadcast::Receiver<ShapeEvent> {
        // Register the receiver BEFORE starting the task so we don't miss events
        let rx = self.inner.sender.subscribe();
        self.ensure_started();
        rx
    }

    /// Convert this stream into a [`futures::Stream`] of message batches.
    ///
    /// Convenience wrapper around [`subscribe`](Self::subscribe).
    pub fn into_stream(&self) -> impl futures::Stream<Item = Vec<Message>> + '_ {
        use futures::StreamExt;
        let rx = self.subscribe();
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| async move {
            match result {
                Ok(ShapeEvent::Batch(msgs)) => Some((*msgs).clone()),
                _ => None,
            }
        })
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    /// True until the first `up-to-date` control message is received.
    pub fn is_loading(&self) -> bool {
        self.inner.state.try_read().map_or(true, |s| s.is_loading)
    }

    /// True once the initial snapshot has been delivered.
    pub fn is_up_to_date(&self) -> bool {
        self.inner
            .state
            .try_read()
            .map_or(false, |s| s.is_up_to_date)
    }

    /// True if the background polling task is running.
    pub fn is_connected(&self) -> bool {
        self.inner.started.load(Ordering::SeqCst) && !self.inner.cancel.is_cancelled()
    }

    /// Current stream offset.
    pub fn last_offset(&self) -> Offset {
        self.inner
            .state
            .try_read()
            .map_or(Offset::Initial, |s| s.offset.clone())
    }

    /// Current shape handle (if any).
    pub fn shape_handle(&self) -> Option<String> {
        self.inner
            .state
            .try_read()
            .ok()
            .and_then(|s| s.handle.clone())
    }

    /// Unix milliseconds of the last successful sync, or `None` if never synced.
    pub fn last_synced_at(&self) -> Option<u64> {
        self.inner
            .state
            .try_read()
            .ok()
            .and_then(|s| s.last_synced_at)
    }

    /// Stop the background polling task.
    pub fn stop(&self) {
        self.inner.cancel.cancel();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn ensure_started(&self) {
        if self
            .inner
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let inner = self.inner.clone();
            tokio::spawn(async move { run_loop(inner).await });
        }
    }
}

impl Drop for ShapeStream {
    fn drop(&mut self) {
        // Cancel the background task when the ShapeStream is dropped.
        self.inner.cancel.cancel();
    }
}

// ── Background polling loop ───────────────────────────────────────────────────

async fn run_loop(inner: Arc<ShapeStreamInner>) {
    // Build the fetch backend (optionally with backoff)
    let fetcher: Arc<dyn Fetcher> = inner.options.fetcher.clone().unwrap_or_else(|| {
        Arc::new(BackoffFetcher::new(
            ReqwestFetcher::new(),
            inner.options.backoff.clone(),
        ))
    });

    loop {
        if inner.cancel.is_cancelled() {
            break;
        }

        // Build the request URL from current state
        let url = {
            let state = inner.state.read().await;
            match build_url(&inner.options, &state) {
                Ok(u) => u,
                Err(e) => {
                    publish_error(&inner, e.to_string()).await;
                    return;
                }
            }
        };

        let req = ElectricRequest {
            url,
            headers: inner.options.headers.clone(),
        };

        // Perform the fetch (potentially with backoff already applied by the fetcher)
        let result = tokio::select! {
            res = fetcher.fetch(req) => res,
            _ = inner.cancel.cancelled() => {
                debug!("ShapeStream cancelled during fetch");
                break;
            }
        };

        match result {
            Err(ElectricError::Fetch {
                status: 409,
                body,
                headers,
                ..
            }) => {
                // Shape rotation: server has invalidated our handle.
                handle_409(&inner, body, headers).await;
                // Loop immediately with the new handle and offset=-1
            }

            Err(e) => {
                publish_error(&inner, e.to_string()).await;
                return;
            }

            Ok(resp) if resp.status == 204 => {
                // No Content (backward compat): up-to-date, no messages
                handle_no_content(&inner).await;
                if !inner.options.subscribe {
                    break;
                }
            }

            Ok(resp) if resp.status == 409 => {
                // Shape rotation via Ok path (e.g. when using a custom fetcher
                // that doesn't convert 409 → Err).
                handle_409(&inner, resp.body, resp.headers).await;
            }

            Ok(resp) if resp.status == 200 => match handle_200(&inner, resp).await {
                Ok(done) => {
                    if done {
                        break;
                    }
                }
                Err(e) => {
                    publish_error(&inner, e.to_string()).await;
                    return;
                }
            },

            Ok(resp) => {
                // Unexpected status (e.g. 400, 401, 404)
                publish_error(&inner, format!("Unexpected HTTP status {}", resp.status)).await;
                return;
            }
        }
    }

    // Publish a clean-close event
    let _ = inner.sender.send(ShapeEvent::Closed);
}

// ── Per-response handlers ─────────────────────────────────────────────────────

/// Handle a 200 OK response: parse headers + messages, update state, publish.
///
/// Returns `Ok(true)` when the loop should stop (reached up-to-date and
/// `subscribe = false`), `Ok(false)` to continue.
async fn handle_200(
    inner: &ShapeStreamInner,
    resp: crate::fetch::ElectricResponse,
) -> Result<bool, ElectricError> {
    // ── Parse Electric response headers ──────────────────────────────────────
    let new_handle = resp.headers.get(SHAPE_HANDLE_HEADER).cloned();
    let new_offset: Option<Offset> = resp
        .headers
        .get(CHUNK_LAST_OFFSET_HEADER)
        .and_then(|s| s.parse().ok());
    let new_schema: Option<Schema> = resp
        .headers
        .get(SHAPE_SCHEMA_HEADER)
        .and_then(|s| serde_json::from_str(s).ok());
    let new_cursor = resp.headers.get(LIVE_CACHE_BUSTER_HEADER).cloned();
    let header_up_to_date = resp
        .headers
        .contains_key(CHUNK_UP_TO_DATE_HEADER.to_lowercase().as_str());

    // ── Update state ──────────────────────────────────────────────────────────
    {
        let mut state = inner.state.write().await;
        if let Some(h) = new_handle {
            state.handle = Some(h);
        }
        if let Some(o) = new_offset {
            state.offset = o;
        }
        if let Some(s) = new_schema {
            state.schema = s;
        }
        if let Some(c) = new_cursor {
            state.cursor = Some(c);
        }
    }

    // ── Parse messages ────────────────────────────────────────────────────────
    // Read schema under a shared lock for parsing
    let schema = inner.state.read().await.schema.clone();

    let messages = parse_response_body(&resp.body, &schema, &inner.options.parser)?;

    let has_up_to_date = header_up_to_date || messages.iter().any(|m| m.is_up_to_date());

    // ── Publish ───────────────────────────────────────────────────────────────
    if !messages.is_empty() {
        let _ = inner.sender.send(ShapeEvent::Batch(Arc::new(messages)));
    }

    // ── Post-up-to-date state ─────────────────────────────────────────────────
    if has_up_to_date {
        let subscribe = inner.options.subscribe;
        {
            let mut state = inner.state.write().await;
            state.is_loading = false;
            state.is_up_to_date = true;
            state.live = subscribe; // switch to live long-polling if subscribing
            state.last_synced_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_millis() as u64,
            );
        }

        if !subscribe {
            return Ok(true); // stop
        }
    }

    Ok(false) // continue
}

/// Handle a 409 Must-Refetch response.
async fn handle_409(inner: &ShapeStreamInner, body: String, headers: HashMap<String, String>) {
    warn!("Received 409 must-refetch; resetting shape state");

    // Server may provide a new handle in the response headers
    let new_handle = headers.get(SHAPE_HANDLE_HEADER).cloned();

    // Parse messages from the 409 body (may contain a `must-refetch` control message)
    let messages: Vec<Message> = if body.is_empty() {
        vec![]
    } else {
        // Schema might be stale — parse without it (values pass through as strings)
        parse_response_body(&body, &Schema::new(), &inner.options.parser).unwrap_or_default()
    };

    // Reset streaming state
    {
        let mut state = inner.state.write().await;
        state.offset = Offset::Initial;
        state.handle = new_handle;
        state.schema = Schema::new();
        state.cursor = None;
        state.is_up_to_date = false;
        state.is_loading = true;
        state.live = false;
    }

    // Still publish the must-refetch messages so Shape can clear its data
    if !messages.is_empty() {
        let _ = inner.sender.send(ShapeEvent::Batch(Arc::new(messages)));
    }
}

/// Handle a 204 No Content response (backward compatibility mode).
async fn handle_no_content(inner: &ShapeStreamInner) {
    let mut state = inner.state.write().await;
    state.is_loading = false;
    state.is_up_to_date = true;
    state.last_synced_at = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64,
    );
}

async fn publish_error(inner: &ShapeStreamInner, msg: String) {
    *inner.last_error.lock().await = Some(msg.clone());
    let _ = inner.sender.send(ShapeEvent::Error(msg));
}

// ── URL builder ───────────────────────────────────────────────────────────────

fn build_url(options: &ShapeStreamOptions, state: &StreamState) -> Result<String, ElectricError> {
    let mut url = url::Url::parse(&options.url)?;

    {
        let mut q = url.query_pairs_mut();

        // Table
        q.append_pair(TABLE_QUERY_PARAM, &options.table);

        // Offset
        q.append_pair(OFFSET_QUERY_PARAM, &state.offset.to_string());

        // Handle (required when offset is not initial)
        if !matches!(state.offset, Offset::Initial) {
            if let Some(h) = &state.handle {
                q.append_pair(SHAPE_HANDLE_QUERY_PARAM, h);
            }
        }

        // Live mode
        if state.live {
            q.append_pair(LIVE_QUERY_PARAM, "true");
            if let Some(c) = &state.cursor {
                q.append_pair(LIVE_CACHE_BUSTER_QUERY_PARAM, c);
            }
        } else {
            q.append_pair(LIVE_QUERY_PARAM, "false");
        }

        // Random cache-buster (prevents stale CDN responses)
        q.append_pair(CACHE_BUSTER_QUERY_PARAM, &Uuid::new_v4().to_string());

        // Optional WHERE clause
        if let Some(w) = &options.where_clause {
            q.append_pair(WHERE_QUERY_PARAM, w);
        }

        // Optional positional params for WHERE clause
        for (k, v) in &options.params {
            q.append_pair(&format!("{}[{}]", WHERE_PARAMS_PARAM, k), v);
        }

        // Optional column subset
        if let Some(cols) = &options.columns {
            q.append_pair(COLUMNS_QUERY_PARAM, &cols.join(","));
        }

        // Replica mode (only emit if non-default)
        if options.replica == Replica::Full {
            q.append_pair(REPLICA_PARAM, "full");
        }
    }

    Ok(url.to_string())
}

// ── Message parsing ───────────────────────────────────────────────────────────

/// Parse a JSON array of Electric messages, applying the parser to `value` and
/// `old_value` fields of each change message.
fn parse_response_body(
    body: &str,
    schema: &Schema,
    parser: &Parser,
) -> Result<Vec<Message>, ElectricError> {
    if body.is_empty() {
        return Ok(vec![]);
    }

    // First pass: deserialise the raw JSON array.  At this point `value` fields
    // inside change messages still hold raw strings.
    let mut messages: Vec<Message> = serde_json::from_str(body)?;

    // Second pass: apply type parsing to change messages.
    if !schema.is_empty() {
        for msg in &mut messages {
            if let Message::Change(change) = msg {
                change.value = parser.parse_row(&change.value, schema)?;
                if let Some(old) = &change.old_value {
                    change.old_value = Some(parser.parse_row(old, schema)?);
                }
            }
        }
    }

    Ok(messages)
}
