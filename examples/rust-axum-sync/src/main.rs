//! Minimal Axum server that keeps a live-synced view of a Postgres table via
//! Electric and serves the current rows as JSON.
//!
//! ## Running
//!
//! 1. Start Electric + Postgres (see AGENTS.md):
//!    ```sh
//!    cd packages/sync-service/dev
//!    docker compose -f docker-compose.yml -f docker-compose-electric.yml up --wait postgres electric
//!    ```
//!
//! 2. Run the example:
//!    ```sh
//!    ELECTRIC_URL=http://localhost:3000 \
//!    ELECTRIC_TABLE=todos \
//!    cargo run --manifest-path examples/rust-axum-sync/Cargo.toml
//!    ```
//!
//! 3. Query the synced data:
//!    ```sh
//!    curl http://localhost:3001/todos
//!    curl http://localhost:3001/health
//!    ```

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use electric_client::{shape::Shape, ShapeStream, ShapeStreamOptions, Row};
use tokio::sync::RwLock;
use tracing::info;

// ── Shared application state ──────────────────────────────────────────────────

type SharedRows = Arc<RwLock<Vec<Row>>>;

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Initialise structured logging (respects `RUST_LOG` env var).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let electric_url = std::env::var("ELECTRIC_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    let table = std::env::var("ELECTRIC_TABLE")
        .unwrap_or_else(|_| "todos".to_string());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3001".to_string());

    info!(%electric_url, %table, %listen_addr, "Starting rust-axum-sync");

    // ── Spawn the Electric shape subscription ─────────────────────────────────
    let rows: SharedRows = Arc::new(RwLock::new(Vec::new()));

    let stream = ShapeStream::new(ShapeStreamOptions {
        url: format!("{}/v1/shape", electric_url.trim_end_matches('/')),
        table: table.clone(),
        subscribe: true, // keep long-polling for live updates
        ..Default::default()
    })
    .expect("Failed to create ShapeStream");

    let shape = Shape::new(stream);

    // Spawn a task that forwards shape change notifications into `rows`.
    let rows_for_sync = rows.clone();
    let shape_arc = Arc::new(shape);
    let shape_for_task = shape_arc.clone();

    tokio::spawn(async move {
        // Wait for the initial snapshot, then register a subscriber for future changes.
        let initial = shape_for_task.rows().await;
        *rows_for_sync.write().await = initial;
        info!("Initial snapshot loaded");

        let rows_cb = rows_for_sync.clone();
        let _guard = shape_for_task
            .subscribe(move |updated| {
                let rows_cb = rows_cb.clone();
                tokio::spawn(async move {
                    *rows_cb.write().await = (*updated).clone();
                });
            })
            .await;

        // Keep this task alive (and thereby keep the subscription guard alive).
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    });

    // ── Axum router ───────────────────────────────────────────────────────────
    let app = Router::new()
        .route("/todos", get(get_todos))
        .route("/health", get(health))
        .with_state(rows);

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .unwrap_or_else(|e| panic!("Cannot bind to {listen_addr}: {e}"));

    info!("Listening on http://{listen_addr}");
    axum::serve(listener, app).await.unwrap();
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /todos` — returns the current in-memory snapshot as JSON.
async fn get_todos(State(rows): State<SharedRows>) -> Json<Vec<Row>> {
    Json(rows.read().await.clone())
}

/// `GET /health` — liveness probe.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}
