//! `Shape`: a materialized view over a [`ShapeStream`].
//!
//! Maintains an in-memory `BTreeMap<key, Row>` that reflects the current
//! state of the synced Postgres table.  Applies inserts, updates, and deletes
//! as message batches arrive and notifies user-registered subscribers after
//! each `up-to-date` batch.
//!
//! Mirrors `packages/typescript-client/src/shape.ts`.

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;

use crate::client::{ShapeEvent, ShapeStream};
use crate::types::{ControlKind, Message, Operation, Row};

// ── Subscriber callback type ──────────────────────────────────────────────────

/// Called on every `up-to-date` batch with the current snapshot of all rows.
///
/// The `Arc<Vec<Row>>` is cheap to clone across threads.
pub type ShapeChangedCallback = Box<dyn Fn(Arc<Vec<Row>>) + Send + Sync + 'static>;

// ── Shared inner state ────────────────────────────────────────────────────────

struct ShapeInner {
    /// Materialised rows keyed by the Electric row key.
    data: RwLock<BTreeMap<String, Row>>,
    /// `true` once the first `up-to-date` message is processed.
    up_to_date_tx: watch::Sender<bool>,
    /// Registered change subscribers.
    subscribers: RwLock<Vec<ShapeChangedCallback>>,
    /// Last error message, if any.
    error: RwLock<Option<String>>,
}

// ── Shape ─────────────────────────────────────────────────────────────────────

/// A materialized, subscribable view over a [`ShapeStream`].
///
/// # Example
///
/// ```rust,no_run
/// use electric_client::{Shape, ShapeStream, ShapeStreamOptions};
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
///     let shape = Shape::new(stream);
///
///     // Async: waits until the initial snapshot is fully loaded.
///     let rows = shape.rows().await;
///     println!("{} todos", rows.len());
///
///     // Sync: returns whatever is in memory right now.
///     let current = shape.current_rows();
///     println!("{} todos (sync)", current.len());
/// }
/// ```
pub struct Shape {
    inner: Arc<ShapeInner>,
    up_to_date_rx: watch::Receiver<bool>,
    /// Keeps the underlying stream (and its polling loop) alive for the
    /// lifetime of the `Shape`. Dropping the stream cancels the loop.
    _stream: ShapeStream,
    /// Background task handle (kept alive by the struct).
    _task: JoinHandle<()>,
}

impl Shape {
    /// Create a new `Shape` backed by the given `ShapeStream`.
    ///
    /// Immediately subscribes to the stream and spawns a background task to
    /// process message batches.
    pub fn new(stream: ShapeStream) -> Self {
        let (up_to_date_tx, up_to_date_rx) = watch::channel(false);

        let inner = Arc::new(ShapeInner {
            data: RwLock::new(BTreeMap::new()),
            up_to_date_tx,
            subscribers: RwLock::new(Vec::new()),
            error: RwLock::new(None),
        });

        // Subscribe BEFORE starting the task to avoid missing early events
        let mut rx = stream.subscribe();
        let inner_clone = inner.clone();

        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ShapeEvent::Batch(msgs)) => {
                        process_batch(&inner_clone, &msgs).await;
                    }
                    Ok(ShapeEvent::Error(e)) => {
                        *inner_clone.error.write().await = Some(e);
                        // Signal up-to-date so any `rows().await` wakers unblock
                        let _ = inner_clone.up_to_date_tx.send(true);
                        break;
                    }
                    Ok(ShapeEvent::Closed) => {
                        // Signal up-to-date so `rows().await` wakers unblock
                        let _ = inner_clone.up_to_date_tx.send(true);
                        break;
                    }
                    Err(_) => {
                        // Sender dropped (stream stopped)
                        let _ = inner_clone.up_to_date_tx.send(true);
                        break;
                    }
                }
            }
        });

        Self {
            inner,
            up_to_date_rx,
            _stream: stream,
            _task: task,
        }
    }

    // ── Data accessors ────────────────────────────────────────────────────────

    /// Async: waits until the initial snapshot is fully loaded, then returns
    /// all rows as a `Vec`.
    pub async fn rows(&self) -> Vec<Row> {
        // Wait until is_up_to_date becomes true (race-condition-safe)
        let mut rx = self.up_to_date_rx.clone();
        let _ = rx.wait_for(|&v| v).await;
        self.inner.data.read().await.values().cloned().collect()
    }

    /// Sync: returns whatever rows are in memory right now (may be incomplete
    /// if the initial snapshot has not yet arrived).
    pub fn current_rows(&self) -> Vec<Row> {
        match self.inner.data.try_read() {
            Ok(data) => data.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// True once the initial snapshot has been delivered.
    pub fn is_up_to_date(&self) -> bool {
        *self.up_to_date_rx.borrow()
    }

    /// Last error encountered by the underlying stream, if any.
    pub fn error(&self) -> Option<String> {
        self.inner.error.try_read().ok().and_then(|e| e.clone())
    }

    // ── Subscribe ─────────────────────────────────────────────────────────────

    /// Register a callback to be called with the full row snapshot after each
    /// `up-to-date` batch.
    ///
    /// The callback is called with an `Arc<Vec<Row>>` (cheap to clone).
    /// Returns a guard whose `Drop` impl deregisters the callback.
    pub async fn subscribe<F>(&self, cb: F) -> SubscriptionGuard
    where
        F: Fn(Arc<Vec<Row>>) + Send + Sync + 'static,
    {
        let mut subs = self.inner.subscribers.write().await;
        let id = subs.len(); // simple monotonic ID
        subs.push(Box::new(cb));
        SubscriptionGuard {
            inner: self.inner.clone(),
            id,
        }
    }

    /// Number of registered subscribers.
    pub async fn num_subscribers(&self) -> usize {
        self.inner.subscribers.read().await.len()
    }
}

// ── Subscription guard ────────────────────────────────────────────────────────

/// Deregisters a callback when dropped.
pub struct SubscriptionGuard {
    inner: Arc<ShapeInner>,
    id: usize,
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        let inner = self.inner.clone();
        let id = self.id;
        if let Ok(mut subs) = inner.subscribers.try_write() {
            if id < subs.len() {
                // Replace with a no-op rather than removing to keep IDs stable.
                subs[id] = Box::new(|_| {});
            }
        };
        // `inner` is dropped here, after the guard from try_write is released.
        drop(inner);
    }
}

// ── Batch processor ───────────────────────────────────────────────────────────

/// Apply a batch of messages to the materialised data map.
async fn process_batch(inner: &ShapeInner, messages: &[Message]) {
    let mut has_up_to_date = false;
    let mut has_must_refetch = false;

    {
        let mut data = inner.data.write().await;

        for msg in messages {
            match msg {
                Message::Control(c) => match c.control {
                    ControlKind::UpToDate => has_up_to_date = true,
                    ControlKind::MustRefetch => {
                        has_must_refetch = true;
                        data.clear();
                    }
                },
                Message::Change(change) => {
                    apply_change(&mut data, change);
                }
            }
        }
    }

    if has_must_refetch {
        // Don't signal up-to-date yet; the stream will re-sync from scratch
        return;
    }

    if has_up_to_date {
        // Collect current rows for subscriber callbacks
        let rows: Arc<Vec<Row>> = {
            let data = inner.data.read().await;
            Arc::new(data.values().cloned().collect())
        };

        // Signal up-to-date
        let _ = inner.up_to_date_tx.send(true);

        // Notify all subscribers
        let subs = inner.subscribers.read().await;
        for cb in subs.iter() {
            cb(rows.clone());
        }
    }
}

/// Apply a single change message to the data map.
fn apply_change(data: &mut BTreeMap<String, Row>, change: &crate::types::ChangeMessage) {
    match change.headers.operation {
        Operation::Insert => {
            data.insert(change.key.clone(), change.value.clone());
        }
        Operation::Update => {
            let entry = data.entry(change.key.clone()).or_insert_with(Row::new);
            // Merge changed columns into the existing row
            for (k, v) in &change.value {
                entry.insert(k.clone(), v.clone());
            }
        }
        Operation::Delete => {
            data.remove(&change.key);
        }
    }
}
