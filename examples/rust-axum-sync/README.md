# rust-axum-sync

A minimal example of using the [`electric-client`](../../packages/rust-client) Rust crate to keep an in-memory snapshot of a Postgres table in sync with [Electric](https://electric-sql.com), and serve it as JSON via [Axum](https://github.com/tokio-rs/axum).

## What it does

1. Subscribes to an Electric shape (a Postgres table) using the `electric-client` crate.
2. Applies incoming inserts, updates, and deletes to an in-memory `Vec<Row>`.
3. Exposes the current snapshot via two HTTP endpoints:
   - `GET /todos` – returns all synced rows as a JSON array.
   - `GET /health` – returns `{"status":"ok"}`.

## Prerequisites

- Rust toolchain (1.75+): https://rustup.rs
- A running Electric + Postgres instance (see instructions below)

## Quick start

### 1. Start Electric and Postgres

```sh
# From the repo root
cd packages/sync-service/dev
docker compose -f docker-compose.yml -f docker-compose-electric.yml \
  up --wait postgres electric
```

This starts:

- PostgreSQL at `localhost:54321`
- Electric at `http://localhost:3000`

### 2. Create a table and seed some data

```sh
psql "host=localhost port=54321 user=postgres password=password dbname=electric" \
  -c "CREATE TABLE todos (id SERIAL PRIMARY KEY, text TEXT NOT NULL, done BOOLEAN NOT NULL DEFAULT false)"

psql "host=localhost port=54321 user=postgres password=password dbname=electric" \
  -c "INSERT INTO todos (text) VALUES ('Buy milk'), ('Write Rust code'), ('Sync with Electric')"
```

### 3. Run the example

```sh
# From the repo root
ELECTRIC_URL=http://localhost:3000 \
ELECTRIC_TABLE=todos \
cargo run --manifest-path examples/rust-axum-sync/Cargo.toml
```

### 4. Query the synced data

```sh
curl http://localhost:3001/todos
# [{"done":false,"id":"1","text":"Buy milk"}, ...]

curl http://localhost:3001/health
# {"status":"ok"}
```

Live inserts/updates/deletes to Postgres are automatically reflected within milliseconds.

## Environment variables

| Variable         | Default                 | Description                           |
| ---------------- | ----------------------- | ------------------------------------- |
| `ELECTRIC_URL`   | `http://localhost:3000` | Base URL of the Electric sync service |
| `ELECTRIC_TABLE` | `todos`                 | Postgres table to subscribe to        |
| `LISTEN_ADDR`    | `0.0.0.0:3001`          | Address for the Axum HTTP server      |
| `RUST_LOG`       | `info`                  | Log level (uses `tracing-subscriber`) |

## Project structure

```
examples/rust-axum-sync/
├── Cargo.toml       # Crate manifest (depends on electric-client)
├── package.json     # npm scripts for convenience
└── src/
    └── main.rs      # Axum server + Electric shape subscription
```

## How it works

```
Electric (HTTP long-poll)
        │
        ▼
  ShapeStream  ──subscribe()──►  broadcast::channel
        │
        ▼
   Shape (in-memory BTreeMap)
        │
        ▼  on_change callback
  Arc<RwLock<Vec<Row>>>   (shared state)
        │
        ▼
   Axum GET /todos   ──►  JSON response
```

- `ShapeStream` polls the Electric HTTP API and emits `ShapeEvent::Batch` messages.
- `Shape` materialises those events into a `BTreeMap<key, Row>`.
- A `subscribe()` callback copies the snapshot into a shared `Arc<RwLock<Vec<Row>>>`.
- The Axum handler reads from that `RwLock` on each request — zero database queries needed.
