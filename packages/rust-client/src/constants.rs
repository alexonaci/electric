//! Protocol constants: HTTP header names and query parameter names used by the
//! Electric sync protocol. Mirrors `packages/typescript-client/src/constants.ts`.

// ── Response headers ──────────────────────────────────────────────────────────

/// Response header carrying the next cursor for CDN cache-busting on live requests.
pub const LIVE_CACHE_BUSTER_HEADER: &str = "electric-cursor";

/// Response header carrying the shape handle (opaque ID for resuming a shape).
pub const SHAPE_HANDLE_HEADER: &str = "electric-handle";

/// Response header carrying the latest offset included in this response.
pub const CHUNK_LAST_OFFSET_HEADER: &str = "electric-offset";

/// Response header carrying the JSON schema (column name → `ColumnInfo`).
pub const SHAPE_SCHEMA_HEADER: &str = "electric-schema";

/// Response header present when the response ends with an `up-to-date` control message.
pub const CHUNK_UP_TO_DATE_HEADER: &str = "electric-up-to-date";

// ── Query parameters ──────────────────────────────────────────────────────────

/// Query param: comma-separated list of columns to include in the shape.
pub const COLUMNS_QUERY_PARAM: &str = "columns";

/// Query param: cursor returned by the server; sent back on live requests for CDN
/// cache coherence.
pub const LIVE_CACHE_BUSTER_QUERY_PARAM: &str = "cursor";

/// Query param: previously expired shape handle, sent to bypass CDN cache.
pub const EXPIRED_HANDLE_QUERY_PARAM: &str = "expired_handle";

/// Query param: the shape handle obtained from a previous response.
pub const SHAPE_HANDLE_QUERY_PARAM: &str = "handle";

/// Query param: `"true"` to long-poll for live changes; `"false"` for one-shot snapshot.
pub const LIVE_QUERY_PARAM: &str = "live";

/// Query param: current stream offset (`"-1"` for initial, `"{tx}_{op}"` otherwise).
pub const OFFSET_QUERY_PARAM: &str = "offset";

/// Query param: root Postgres table to subscribe to.
pub const TABLE_QUERY_PARAM: &str = "table";

/// Query param: SQL WHERE clause string for server-side filtering.
pub const WHERE_QUERY_PARAM: &str = "where";

/// Query param: replica mode (`"default"` or `"full"`).
pub const REPLICA_PARAM: &str = "replica";

/// Query param: positional WHERE clause parameter values (encoded as `params[1]=…`).
pub const WHERE_PARAMS_PARAM: &str = "params";

/// Query param: enables SSE streaming (not used in MVP long-poll mode).
pub const LIVE_SSE_QUERY_PARAM: &str = "live_sse";

/// Query param: log mode (`"full"` includes initial snapshot; `"changes_only"` skips it).
pub const LOG_MODE_QUERY_PARAM: &str = "log";

/// Query param: random UUID appended to every request to bypass stale CDN responses.
pub const CACHE_BUSTER_QUERY_PARAM: &str = "cache-buster";

/// All query parameters that are part of the Electric protocol (forwarded by proxies).
pub const ELECTRIC_PROTOCOL_QUERY_PARAMS: &[&str] = &[
    LIVE_QUERY_PARAM,
    LIVE_SSE_QUERY_PARAM,
    SHAPE_HANDLE_QUERY_PARAM,
    OFFSET_QUERY_PARAM,
    LIVE_CACHE_BUSTER_QUERY_PARAM,
    EXPIRED_HANDLE_QUERY_PARAM,
    LOG_MODE_QUERY_PARAM,
    CACHE_BUSTER_QUERY_PARAM,
];

/// Reserved parameter names that users cannot override in custom `params`.
pub const RESERVED_PARAMS: &[&str] = &[
    LIVE_CACHE_BUSTER_QUERY_PARAM,
    SHAPE_HANDLE_QUERY_PARAM,
    LIVE_QUERY_PARAM,
    OFFSET_QUERY_PARAM,
    CACHE_BUSTER_QUERY_PARAM,
];
