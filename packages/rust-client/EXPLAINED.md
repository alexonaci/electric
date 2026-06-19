# `electric-client` Explained — for TypeScript Developers

This guide explains the Rust code in this crate **assuming you know TypeScript but not Rust**.
It has two parts:

1. **[Rust concepts primer](#part-1--rust-concepts-primer)** — the handful of Rust ideas you need, each mapped to a TS equivalent.
2. **[Per-file walkthrough](#part-2--per-file-walkthrough)** — what every source file does and why.

> TL;DR of the architecture:
> `ShapeStream` long-polls Electric's HTTP API and emits batches of messages.
> `Shape` consumes those batches and keeps an in-memory table of rows you can read.
> Everything is async (Tokio), and shared state is wrapped in thread-safe smart pointers.

---

## Part 1 — Rust concepts primer

### 1.1 `Result<T, E>` and `Option<T>` — no `null`, no `throw`

Rust has **no `null`** and **no exceptions**. Instead:

| TypeScript             | Rust           | Meaning                                          |
| ---------------------- | -------------- | ------------------------------------------------ |
| `T \| undefined`       | `Option<T>`    | a value that may be absent (`Some(x)` or `None`) |
| `try/catch` / throwing | `Result<T, E>` | success (`Ok(x)`) or failure (`Err(e)`)          |

```rust
fn find_user(id: u32) -> Option<User> { ... }   // maybe a user
fn save(u: &User) -> Result<(), DbError> { ... } // ok or an error
```

The `?` operator is like an early-return for errors. This:

```rust
let row = parse_row(raw)?;   // if Err, return that Err from this function
```

is roughly the TS:

```ts
const row = parseRow(raw) // but if it failed, it would throw and bubble up
```

### 1.2 Ownership & borrowing (the famous one)

Every value has exactly **one owner**. When the owner goes out of scope, the value is freed (no garbage collector). You can lend out references:

- `&T` — a **shared/immutable borrow** (many allowed at once) — like a `readonly` reference.
- `&mut T` — an **exclusive/mutable borrow** (only one at a time).

This is why you'll see `&self` (read-only method) vs `&mut self` (mutating method). It's the compiler enforcing "no data races" at compile time. In TS you never think about this because the GC and single-threaded event loop hide it.

### 1.3 `Arc<T>` — shared ownership across threads

Sometimes multiple tasks need to own the same data. `Arc<T>` = **A**tomically **R**eference **C**ounted pointer. Think of it as a thread-safe shared pointer; cloning an `Arc` just bumps a counter (cheap), and the data is freed when the last `Arc` drops.

```rust
let shared = Arc::new(state);
let clone_for_task = shared.clone(); // same underlying data, +1 refcount
```

TS analogy: passing the same object reference into multiple closures — except Rust needs `Arc` to make it safe across OS threads.

### 1.4 `Mutex` / `RwLock` — guarding shared mutable data

Because Rust forbids shared _mutable_ access by default, to mutate data shared via `Arc`, you wrap it in a lock:

- `RwLock<T>` — many readers **or** one writer (like a readers-writer lock).
- `Mutex<T>` — one accessor at a time.

We use **Tokio's async versions** (`tokio::sync::RwLock`), so acquiring a lock is `.await`ed instead of blocking the thread:

```rust
let data = state.read().await;     // shared read lock
let mut data = state.write().await; // exclusive write lock
```

> ⚠️ Gotcha we hit: `blocking_read()` **panics** if called inside an async task. Use `try_read()` (non-blocking, returns a `Result`) for synchronous getter methods.

### 1.5 `async` / `.await` and Tokio

Just like TS `async`/`await`. Differences:

- Rust needs a **runtime** to drive futures. We use **Tokio** (`#[tokio::main]`, `tokio::spawn`).
- `tokio::spawn(async { ... })` ≈ kicking off a detached promise / background task.
- A Rust `Future` does **nothing until awaited or spawned** (lazy), unlike a JS Promise which starts immediately.

### 1.6 Channels — passing messages between tasks

Instead of shared mutable state, tasks often communicate via channels (like Go, or a typed event emitter):

- `broadcast::channel` — **one producer → many subscribers**, every subscriber gets every message. We use it to fan out message batches to all listeners.
- `watch::channel` — holds a **single latest value**; readers can `.await` until it changes. We use it as an "are we up-to-date yet?" flag.

### 1.7 `enum` — way more powerful than TS enums

Rust enums are **tagged unions** (like TS discriminated unions), and each variant can carry data:

```rust
enum Message {
    Change(ChangeMessage),   // carries a struct
    Control(ControlMessage),
}
```

You destructure them with `match` (like an exhaustive `switch` the compiler forces you to cover):

```rust
match msg {
    Message::Change(c) => { /* use c */ }
    Message::Control(c) => { /* use c */ }
}
```

### 1.8 `trait` — like an interface

A `trait` is an interface. `impl Trait for Type` provides the implementation.

```rust
#[async_trait]
trait Fetcher {
    async fn fetch(&self, req: ElectricRequest) -> Result<ElectricResponse, ElectricError>;
}
```

`dyn Fetcher` is a **trait object** = "any type implementing `Fetcher`" (like programming to an interface for dependency injection / mocking). `Arc<dyn Fetcher>` is a shared pointer to one.

### 1.9 `struct`, `impl`, and derive macros

- `struct` = an object shape (like a TS `interface`/`class` fields).
- `impl Foo { fn bar(&self) {} }` = methods on that struct (the `class` body).
- `#[derive(Debug, Clone)]` = auto-generate boilerplate (here: printable + cloneable). Like decorators that codegen methods.

### 1.10 Lifetimes & `Drop`

- `Drop` is a destructor — code that runs when a value is freed (like a `finally` / cleanup). We use it to cancel background tasks and to unsubscribe.
- `'static`, `'_` etc. are **lifetimes** (how long references are valid). You'll mostly see them in signatures; you rarely write them by hand here.

---

## Part 2 — Per-file walkthrough

Source files live in [src/](src/). Tests live in [tests/](tests/).

```
src/
├── lib.rs        ← crate entry point: module list + public re-exports
├── constants.rs  ← protocol string constants (header & query-param names)
├── error.rs      ← the ElectricError enum (all failure modes)
├── types.rs      ← core data types: Message, Offset, Row, Schema, ...
├── parser.rs     ← converts Postgres text values → typed JSON values
├── fetch.rs      ← HTTP layer: Fetcher trait, reqwest impl, retry/backoff
├── client.rs     ← ShapeStream: the long-poll loop (the heart)
└── shape.rs      ← Shape: materialized in-memory table over a ShapeStream
```

Dependency direction (top depends on bottom):

```
shape.rs  →  client.rs  →  fetch.rs  →  (reqwest)
              │             │
              ▼             ▼
           types.rs  ←  parser.rs  →  types.rs
              ▲
       constants.rs, error.rs (used everywhere)
```

---

### `lib.rs` — the crate's front door

[src/lib.rs](src/lib.rs)

- The `//!` comments at the top are **crate-level docs** (rendered by `cargo doc`). The fenced ` ```rust ` block is a **doctest** — Rust actually compiles and runs it during `cargo test`.
- `pub mod client;` etc. declare the modules (files) that make up the crate. Like a barrel `index.ts` listing the files.
- `pub use client::{ShapeStream, ...};` **re-exports** the important types so users write `electric_client::ShapeStream` instead of `electric_client::client::ShapeStream`. Exactly like re-exporting from a barrel file.

There's no logic here — it's pure wiring.

---

### `constants.rs` — protocol vocabulary

[src/constants.rs](src/constants.rs)

Just named string constants for Electric's HTTP header names (`electric-handle`, `electric-offset`, …) and query parameters (`table`, `offset`, `live`, …).

```rust
pub const SHAPE_HANDLE_HEADER: &str = "electric-handle";
```

- `&str` is a **string slice** — an immutable borrowed string (think `readonly string`). String literals are `&'static str` (live for the whole program).
- `RESERVED_PARAMS: &[&str]` is a slice (array view) of those names, used to reject user-supplied params that would clash with protocol ones.

Why a whole file? So the magic strings live in exactly one place and match the TypeScript client's `constants.ts`.

---

### `error.rs` — one error type to rule them all

[src/error.rs](src/error.rs)

Defines `ElectricError`, an `enum` with one variant per failure mode (network error, bad offset, missing header, reserved param, etc.).

- `#[derive(thiserror::Error)]` auto-implements the standard `Error` trait and generates the human-readable messages from the `#[error("...")]` attributes. (`thiserror` is the idiomatic crate for library error enums.)
- It's `Clone` so errors can be copied into multiple subscribers. Because `reqwest::Error` is **not** `Clone`, we convert network failures into a `Network(String)` variant (store the message, not the original error).
- `impl From<serde_json::Error> for ElectricError` lets the `?` operator auto-convert JSON errors into our error type. `From` is Rust's standard "convert A into B" trait.

TS analogy: a custom `Error` subclass hierarchy collapsed into a single discriminated union, where `?` auto-wraps lower-level errors.

---

### `types.rs` — the data model

[src/types.rs](src/types.rs)

The core domain types. Key ones:

- **`Row = serde_json::Map<String, Value>`** — a type alias. A row is just a JSON object (column name → JSON value). Like `Record<string, unknown>`.
- **`Offset`** — an `enum` for the client's position in the shape log: `Initial` (`-1`), `Now`, or `At { tx, op }`. It implements:
  - `Display`/`FromStr` to convert to/from the wire strings (`"-1"`, `"0_0"`, `"0_inf"`).
  - `Ord` so positions can be compared.
  - ⚠️ **Protocol gotcha:** Electric's live position uses `op = inf` (e.g. `"0_inf"`). We store that as the sentinel `u64::MAX` so ordering still works and it round-trips back to `"inf"`. Originally `inf` failed to parse and the client got stuck on a stale offset.
- **`Operation`** — `Insert` / `Update` / `Delete`. `#[serde(rename_all = "lowercase")]` makes it (de)serialize as `"insert"` etc.
- **`ChangeHeaders`** — metadata on a change. ⚠️ **Gotcha:** `txids` are JSON **integers** (`[947]`), so the field is `Vec<u64>`, not `Vec<String>`. Getting this wrong made the live loop silently die on a JSON type error.
- **`Message`** — the big `enum`: either a `Change(ChangeMessage)` (a row insert/update/delete) or a `Control(ControlMessage)` (a signal like `up-to-date` / `must-refetch` / `snapshot-end`).
  - It has a **custom `Deserialize`** (the `impl<'de> Deserialize<'de> for Message`) because we must look at the `headers` field to decide which variant to build. This is like writing a custom JSON reviver that inspects a discriminant.
  - `snapshot-end` (end of the initial snapshot) is treated like `up-to-date` for a read client.
- **`Schema` / `ColumnInfo`** — the table schema Electric sends in the `electric-schema` header (column → pg type, nullability, etc.), used by the parser.

The `#[cfg(test)] mod tests { ... }` block at the bottom holds unit tests that only compile in test builds (`#[cfg(test)]` = conditional compilation, like `if (process.env.NODE_ENV === 'test')` but at compile time).

---

### `parser.rs` — Postgres text → typed values

[src/parser.rs](src/parser.rs)

Electric sends column values as **strings** (it's text-based over HTTP). This module converts them to proper JSON types using the schema.

- `Parser` holds a map of custom parse functions keyed by Postgres type. You can register your own (e.g. parse `timestamptz` into a date type).
- `ParseFn = Arc<dyn Fn(&str) -> Value + Send + Sync>` — a **shared, thread-safe function pointer**. `dyn Fn(...)` is a closure type (like `(s: string) => Value`); `Send + Sync` mean "safe to move/share across threads".
- `parse_value` applies built-in rules: `int4` → number, `int8`/`bigint` → kept as **string** (to avoid JS-style precision loss), `bool` → `true/false`, `json`/`jsonb` → parsed JSON, arrays via `pg_array_parse`, everything else → string.
- `parse_row` runs every column of a row through `parse_value`.

TS analogy: a configurable codec that turns the stringly-typed wire format into properly typed objects, with sane defaults and user-overridable hooks.

---

### `fetch.rs` — the HTTP layer (with retries)

[src/fetch.rs](src/fetch.rs)

Abstracts "make an HTTP GET to Electric" so it can be mocked in tests and wrapped with retry logic.

- **`Fetcher` trait** — the interface: `async fn fetch(req) -> Result<ElectricResponse, ElectricError>`. `#[async_trait]` is needed because plain Rust traits can't yet have `async fn` in all positions; this macro desugars it.
- **`ReqwestFetcher`** — the real implementation using the `reqwest` HTTP client. It lowercases header names and converts any `reqwest` error into our `ElectricError::Network(String)`.
- **`BackoffFetcher<F>`** — a **decorator** that wraps any `Fetcher` and adds exponential backoff + jitter:
  - Retries `5xx` and `429` (rate-limited), honoring `Retry-After`.
  - Does **not** retry other `4xx` (e.g. 404, 409) — those are returned immediately so the caller can react (the loop handles `409` as a shape rotation).
  - `204 No Content` is treated as success.
  - `<F: Fetcher>` is a **generic type parameter** (like TS generics `<F extends Fetcher>`), so backoff works over any fetcher, including the mock one.
- `compute_backoff_wait(...)` implements full-jitter exponential backoff (randomized delays to avoid thundering-herd retries).

Unit tests at the bottom use `wiremock` (a mock HTTP server).

---

### `client.rs` — `ShapeStream`, the heart

[src/client.rs](src/client.rs)

This is the long-poll engine. It repeatedly calls Electric, tracks protocol state, and broadcasts message batches.

Key pieces:

- **`ShapeStreamOptions`** — the config struct (url, table, where clause, params, replica mode, parser, optional custom fetcher, `subscribe` flag). It derives `Default`, so callers use `..Default::default()` to fill the rest (like `{ ...defaults, url, table }`).
- **`ShapeEvent`** — what subscribers receive: `Batch(Arc<Vec<Message>>)`, `Error(String)`, or `Closed`. The batch is an `Arc` so fan-out to many subscribers is cheap (no copying).
- **`StreamState`** — mutable protocol state (current `offset`, `handle`, `schema`, `cursor`, `is_loading`, `is_up_to_date`, `live`, …).
- **`ShapeStreamInner`** — the shared guts: `options`, `RwLock<StreamState>`, the `broadcast::Sender`, an `AtomicBool` "started" flag, and a `CancellationToken`. Wrapped in `Arc` so the background task and the handle share it.
- **`ShapeStream`** — the public handle (just an `Arc<ShapeStreamInner>`). Its `Drop` impl cancels the `CancellationToken`, stopping the background loop when you drop the stream.

How it runs:

1. `ShapeStream::new(opts)` validates options and creates the broadcast channel — but **doesn't start polling yet** (lazy).
2. `subscribe()` registers a receiver **then** calls `ensure_started()`. Order matters: subscribing first guarantees you won't miss the first batch. `ensure_started` uses an atomic compare-and-swap to spawn the loop **exactly once**.
3. `into_stream()` adapts the broadcast receiver into a `futures::Stream` (an async iterator) for ergonomic `while let Some(batch) = s.next().await`.
4. The background **`run_loop`**:
   - builds the URL from current state (`build_url`),
   - fetches (racing the fetch against the cancellation token via `tokio::select!` — like `Promise.race`),
   - dispatches on status: `409` → `handle_409` (shape rotation: reset offset to `-1`, adopt the new handle), `204` → up-to-date no-op, `200` → `handle_200`, anything else → error.
5. **`handle_200`** parses the response headers (handle/offset/schema/cursor), updates state, parses the body into `Message`s, broadcasts the batch, and — when an up-to-date marker is seen — flips `is_up_to_date`/`live`. If `subscribe == false`, it returns "done" and the loop stops after the first up-to-date.

> ⚠️ The protocol flow that bit us: the **first** response ends with a `snapshot-end` control and an `electric-offset: 0_0`; the **next** request (still long-poll) returns the real `up-to-date` with an `electric-offset: <lsn>_0`; subsequent live requests carry `offset=<...>&live=true` and **block** until data changes. All three protocol gotchas (txids ints, `0_inf` offset, `snapshot-end`) had to be right for live updates to flow.

`build_url` uses the `url` crate to append query params (auto percent-encoding). `parse_response_body` deserializes the JSON array, then runs the parser over each change message's `value`/`old_value`.

---

### `shape.rs` — `Shape`, the materialized view

[src/shape.rs](src/shape.rs)

Turns the raw event stream into a queryable in-memory table — this is what most apps use.

- **`ShapeInner`** holds the materialized `RwLock<BTreeMap<String, Row>>` (rows keyed by Electric key; `BTreeMap` keeps them sorted — like a `Map` with ordered keys), a `watch` channel as the "up-to-date" flag, the list of subscriber callbacks, and the last error.
- **`Shape`** wraps `Arc<ShapeInner>`, a `watch::Receiver`, the background `JoinHandle`, and — crucially — **`_stream: ShapeStream`**.
  - ⚠️ **Bug we fixed:** if the `ShapeStream` isn't stored here, it gets dropped at the end of `Shape::new`, its `Drop` cancels the poll loop, and **nothing ever syncs**. Keeping `_stream` alive keeps the loop running. (The leading `_` just tells the compiler "intentionally unused name".)
- **`Shape::new(stream)`** subscribes to the stream **first**, then spawns a task that consumes `ShapeEvent`s and applies them via `process_batch`.
- **`process_batch`** applies each change to the map: insert = set, update = **merge** changed columns into the existing row, delete = remove. On an up-to-date control it flips the `watch` flag and fires subscriber callbacks; on `must-refetch` it clears the map.
- **Reading the data:**
  - `rows().await` waits until up-to-date using `watch::Receiver::wait_for(|&v| v)` (race-safe: returns immediately if already up-to-date), then snapshots the map. This is the reliable API.
  - `current_rows()` is a **sync** best-effort read using `try_read()` (returns empty if the lock is momentarily held). Never use `blocking_read()` here — it panics inside the async runtime.
- **`subscribe(cb)`** registers a callback fired after each up-to-date batch and returns a **`SubscriptionGuard`**. When the guard is dropped, its `Drop` impl replaces the callback with a no-op (unsubscribe). This is the RAII pattern: cleanup tied to scope, like a disposable returned from `addEventListener` that auto-removes itself.

---

## Tests at a glance

[tests/](tests/)

- `tests/support/mod.rs` — shared helpers + a `MockFetcher` (in-memory, no network) implementing the `Fetcher` trait, so client/shape logic is tested deterministically.
- `tests/fetch_tests.rs` — `wiremock`-based tests of the real HTTP fetcher + backoff (retries, 4xx vs 5xx, 204).
- `tests/client_tests.rs` — `ShapeStream` behavior: sync, header tracking, 409 rotation, validation, WHERE forwarding.
- `tests/shape_tests.rs` — `Shape` materialization: insert/update-merge/delete/must-refetch/subscribe.
- `tests/integration.rs` — **real** Electric + Postgres (feature-gated behind `--features integration`). These verify the actual wire protocol and caught all three protocol gotchas above.

Run them:

```sh
cargo test                       # 77 mock/unit tests, no services needed
# integration (needs Docker services from packages/sync-service/dev):
ELECTRIC_URL=http://localhost:3000/v1/shape \
  cargo test --features integration --test integration -- --test-threads=1
```

---

## Cheat-sheet: Rust symbol → TS meaning

| You see…                 | It means…                                            |
| ------------------------ | ---------------------------------------------------- |
| `&self`                  | read-only method (`this`, no mutation)               |
| `&mut self`              | mutating method                                      |
| `Arc<T>`                 | shared, thread-safe pointer (refcounted)             |
| `RwLock<T>` / `Mutex<T>` | lock guarding shared mutable data                    |
| `Option<T>`              | `T \| undefined` (`Some`/`None`)                     |
| `Result<T, E>`           | success or error (`Ok`/`Err`); `?` = bubble error up |
| `enum` + `match`         | discriminated union + exhaustive switch              |
| `trait` / `dyn Trait`    | interface / value behind an interface                |
| `impl Block`             | the methods of a struct (class body)                 |
| `#[derive(...)]`         | codegen boilerplate (decorators)                     |
| `async fn` + `.await`    | same as TS, but lazy and needs Tokio                 |
| `tokio::spawn`           | start a detached background task                     |
| `Drop`                   | destructor / cleanup (`finally`)                     |
| `'static`, `'a`          | lifetimes (how long a reference is valid)            |
| `Vec<T>`                 | `Array<T>`                                           |
| `&str` / `String`        | borrowed string / owned string                       |
