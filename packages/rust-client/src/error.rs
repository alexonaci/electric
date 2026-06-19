//! Error types for the Electric client.

use std::collections::HashMap;
use thiserror::Error;

/// The unified error type returned by all Electric client operations.
#[derive(Debug, Clone, Error)]
pub enum ElectricError {
    /// An HTTP request failed with a non-retryable status code.
    ///
    /// `status` is the HTTP status code.  `body` is the raw response body
    /// (may be empty).  `headers` are the response headers.
    #[error("HTTP {status} from {url}: {body}")]
    Fetch {
        status: u16,
        body: String,
        headers: HashMap<String, String>,
        url: String,
    },

    /// The fetch was aborted (e.g. the stream's cancellation token was cancelled
    /// while waiting inside the backoff loop).
    #[error("Fetch aborted during backoff")]
    BackoffAborted,

    /// `ShapeStreamOptions::url` was empty.
    #[error("Missing required option: url")]
    MissingShapeUrl,

    /// `ShapeStreamOptions::table` was empty.
    #[error("Missing required option: table")]
    MissingShapeTable,

    /// `handle` is required when `offset` is not `Offset::Initial`.
    #[error("shape handle is required when offset is not -1")]
    MissingShapeHandle,

    /// The user passed a reserved Electric parameter name in custom params.
    #[error("Cannot use reserved Electric parameter names: {0:?}")]
    ReservedParam(Vec<String>),

    /// A column declared NOT NULL received a null value from the server.
    #[error("Column \"{column}\" is NOT NULL but received a null value")]
    ParserNullValue { column: String },

    /// A required Electric response header was absent.
    #[error("Missing required response header: {header}")]
    MissingHeader { header: String },

    /// A network or transport error occurred (connection refused, DNS, TLS, etc.).
    #[error("Network error: {0}")]
    Network(String),

    /// JSON (de)serialization failed.
    #[error("JSON error: {0}")]
    Json(String),

    /// URL parsing failed.
    #[error("URL parse error: {0}")]
    Url(String),

    /// An offset string could not be parsed.
    #[error("Invalid offset string: \"{0}\"")]
    InvalidOffset(String),

    /// The streaming task was closed before the caller's future resolved.
    #[error("Stream closed unexpectedly")]
    StreamClosed,

    /// A generic error message produced inside the streaming loop.
    #[error("{0}")]
    Stream(String),
}

impl From<serde_json::Error> for ElectricError {
    fn from(e: serde_json::Error) -> Self {
        ElectricError::Json(e.to_string())
    }
}

impl From<url::ParseError> for ElectricError {
    fn from(e: url::ParseError) -> Self {
        ElectricError::Url(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_error_display() {
        let e = ElectricError::Fetch {
            status: 409,
            body: "must-refetch".to_string(),
            headers: HashMap::new(),
            url: "http://localhost:3000/v1/shape".to_string(),
        };
        assert!(e.to_string().contains("409"));
        assert!(e.to_string().contains("must-refetch"));
    }

    #[test]
    fn null_value_error_display() {
        let e = ElectricError::ParserNullValue {
            column: "user_id".to_string(),
        };
        assert!(e.to_string().contains("user_id"));
    }

    #[test]
    fn missing_header_error_display() {
        let e = ElectricError::MissingHeader {
            header: "electric-handle".to_string(),
        };
        assert!(e.to_string().contains("electric-handle"));
    }
}
