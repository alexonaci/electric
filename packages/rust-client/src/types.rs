//! Core data types for the Electric sync protocol.
//!
//! Mirrors `packages/typescript-client/src/types.ts`.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::error::ElectricError;

// ── Row / Schema ──────────────────────────────────────────────────────────────

/// A single materialized row: column name → parsed JSON value.
///
/// Values are parsed from Postgres text representations using the [`Parser`](crate::parser::Parser).
pub type Row = serde_json::Map<String, serde_json::Value>;

/// Schema returned in the `electric-schema` response header: column name → column metadata.
pub type Schema = HashMap<String, ColumnInfo>;

/// Postgres column metadata, as returned in the `electric-schema` header.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct ColumnInfo {
    /// Postgres type name, e.g. `"int4"`, `"text"`, `"bool"`, `"timestamptz"`.
    #[serde(rename = "type")]
    pub pg_type: String,
    /// Array dimensionality: 0 = scalar, 1 = 1-D array, 2 = 2-D array, etc.
    #[serde(default)]
    pub dimensions: u32,
    /// Whether the column has a NOT NULL constraint.
    #[serde(default)]
    pub not_null: bool,
    /// Maximum character length (varchar/char).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// Fixed character length (bpchar).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    /// Numeric/timestamp precision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u32>,
    /// Numeric scale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
}

// ── Offset ────────────────────────────────────────────────────────────────────

/// A position in the Electric shape log.
///
/// Serialises to/from the wire format:
/// - `"-1"` → `Offset::Initial` (request full history from the start)
/// - `"now"` → `Offset::Now` (skip history, receive only live changes)
/// - `"{tx}_{op}"` → `Offset::At { tx, op }` (specific log position)
///
/// Electric uses a special operation value `inf` (e.g. `"0_inf"`) to mark the
/// "live" position at the end of a transaction. We represent that internally
/// with the sentinel [`Offset::OP_INF`] (= `u64::MAX`) so that ordering still
/// works (infinity sorts highest) and it round-trips back to `"inf"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Offset {
    /// `-1`: request the full historical snapshot from the beginning.
    Initial,
    /// `"now"`: skip existing data, receive only future live changes.
    Now,
    /// A concrete position `{tx}_{op}` in the replication log.
    At { tx: u64, op: u64 },
}

impl Offset {
    /// Sentinel value used to represent the `inf` operation offset that Electric
    /// emits for the live position (e.g. `"0_inf"`).
    pub const OP_INF: u64 = u64::MAX;
}

impl Default for Offset {
    fn default() -> Self {
        Offset::Initial
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Offset::Initial => write!(f, "-1"),
            Offset::Now => write!(f, "now"),
            Offset::At {
                tx,
                op: Self::OP_INF,
            } => write!(f, "{}_inf", tx),
            Offset::At { tx, op } => write!(f, "{}_{}", tx, op),
        }
    }
}

impl FromStr for Offset {
    type Err = ElectricError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "-1" => Ok(Offset::Initial),
            "now" => Ok(Offset::Now),
            other => {
                // Expected format: "{tx}_{op}"
                let mut parts = other.splitn(2, '_');
                let tx_str = parts
                    .next()
                    .ok_or_else(|| ElectricError::InvalidOffset(other.to_string()))?;
                let op_str = parts
                    .next()
                    .ok_or_else(|| ElectricError::InvalidOffset(other.to_string()))?;
                let tx = tx_str
                    .parse::<u64>()
                    .map_err(|_| ElectricError::InvalidOffset(other.to_string()))?;
                // The op may be the literal "inf" for the live position.
                let op = if op_str == "inf" {
                    Offset::OP_INF
                } else {
                    op_str
                        .parse::<u64>()
                        .map_err(|_| ElectricError::InvalidOffset(other.to_string()))?
                };
                Ok(Offset::At { tx, op })
            }
        }
    }
}

impl Ord for Offset {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match (self, other) {
            (Offset::Initial, Offset::Initial) => Equal,
            (Offset::Initial, _) => Less,
            (_, Offset::Initial) => Greater,
            (Offset::Now, Offset::Now) => Equal,
            (Offset::At { tx: t1, op: o1 }, Offset::At { tx: t2, op: o2 }) => {
                t1.cmp(t2).then(o1.cmp(o2))
            }
            // "now" is conceptually "latest possible"; treat as greater than At
            (Offset::Now, Offset::At { .. }) => Greater,
            (Offset::At { .. }, Offset::Now) => Less,
        }
    }
}

impl PartialOrd for Offset {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'de> Deserialize<'de> for Offset {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse()
            .map_err(|e: ElectricError| de::Error::custom(e.to_string()))
    }
}

// ── Message types ─────────────────────────────────────────────────────────────

/// Row operation type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Insert,
    Update,
    Delete,
}

/// Headers attached to a [`ChangeMessage`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeHeaders {
    pub operation: Operation,
    /// Transaction IDs from the Postgres WAL. Electric sends these as JSON
    /// integers (e.g. `[947]`), matching Postgres `xid8` values.
    #[serde(default)]
    pub txids: Vec<u64>,
    /// LSN string (only present on streamed ops, not initial snapshot rows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsn: Option<String>,
}

/// A row-level change event (insert, update, or delete).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeMessage {
    /// The Electric row key (Postgres primary key encoded as a string).
    pub key: String,
    /// The current column values (post-parse).
    pub value: Row,
    /// The previous values for changed columns (only on updates with `replica=full`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<Row>,
    /// Change metadata (operation type, txids, lsn).
    pub headers: ChangeHeaders,
}

/// The kind of control event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlKind {
    /// The client has received all available data and is now up-to-date.
    UpToDate,
    /// The shape log was truncated; the client must discard local state and re-sync.
    MustRefetch,
}

/// A control message from the server.
#[derive(Debug, Clone)]
pub struct ControlMessage {
    pub control: ControlKind,
}

/// A single message received from the Electric sync protocol.
///
/// Each HTTP response body is a JSON array of messages.  Messages are
/// either row-level changes or control signals.
#[derive(Debug, Clone)]
pub enum Message {
    Change(ChangeMessage),
    Control(ControlMessage),
}

impl Message {
    /// True if this is an `up-to-date` control message.
    pub fn is_up_to_date(&self) -> bool {
        matches!(self, Message::Control(c) if c.control == ControlKind::UpToDate)
    }

    /// True if this is a `must-refetch` control message.
    pub fn is_must_refetch(&self) -> bool {
        matches!(self, Message::Control(c) if c.control == ControlKind::MustRefetch)
    }
}

/// Custom [`Deserialize`] for `Message` that distinguishes change vs control
/// by inspecting the `headers` field.
impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Deserialise to a raw JSON value first so we can inspect the `headers` field
        // without committing to a concrete type prematurely.
        let raw = serde_json::Value::deserialize(d)?;

        let headers = raw
            .get("headers")
            .ok_or_else(|| de::Error::missing_field("headers"))?;

        if let Some(control_val) = headers.get("control") {
            let control_str = control_val.as_str().unwrap_or("");
            let kind = match control_str {
                "up-to-date" => ControlKind::UpToDate,
                // `snapshot-end` marks the end of the initial snapshot phase.
                // For a read-path client this means the initial data set is
                // fully loaded, so we treat it like `up-to-date`.
                "snapshot-end" => ControlKind::UpToDate,
                "must-refetch" => ControlKind::MustRefetch,
                other => {
                    // Unknown control type — treat as up-to-date to stay safe.
                    tracing::warn!(
                        control = other,
                        "Unknown control message type; treating as up-to-date"
                    );
                    ControlKind::UpToDate
                }
            };
            Ok(Message::Control(ControlMessage { control: kind }))
        } else if headers.get("operation").is_some() {
            // Deserialise the raw value as a ChangeMessage.
            let change: ChangeMessage = serde_json::from_value(raw).map_err(de::Error::custom)?;
            Ok(Message::Change(change))
        } else {
            // Unknown message shape — skip gracefully.
            tracing::debug!("Skipping unknown message shape");
            // Return as up-to-date so the loop continues.
            Ok(Message::Control(ControlMessage {
                control: ControlKind::UpToDate,
            }))
        }
    }
}

/// Replica mode: controls how much data is included in update/delete messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Replica {
    /// Changed columns only in updates; PK only in deletes. (Default)
    #[default]
    Default,
    /// Full row in updates (including `old_value`); full row in deletes.
    Full,
}

impl fmt::Display for Replica {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Replica::Default => write!(f, "default"),
            Replica::Full => write!(f, "full"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Offset tests ──────────────────────────────────────────────────────────

    #[test]
    fn offset_parse_initial() {
        let o: Offset = "-1".parse().unwrap();
        assert_eq!(o, Offset::Initial);
    }

    #[test]
    fn offset_parse_now() {
        let o: Offset = "now".parse().unwrap();
        assert_eq!(o, Offset::Now);
    }

    #[test]
    fn offset_parse_at() {
        let o: Offset = "26800584_4".parse().unwrap();
        assert_eq!(
            o,
            Offset::At {
                tx: 26800584,
                op: 4
            }
        );
    }

    #[test]
    fn offset_parse_zero() {
        let o: Offset = "0_0".parse().unwrap();
        assert_eq!(o, Offset::At { tx: 0, op: 0 });
    }

    #[test]
    fn offset_parse_error() {
        let result: Result<Offset, _> = "not_valid_offset_xyz".parse();
        // "not" is not a valid u64
        assert!(result.is_err());
    }

    #[test]
    fn offset_display() {
        assert_eq!(Offset::Initial.to_string(), "-1");
        assert_eq!(Offset::Now.to_string(), "now");
        assert_eq!(Offset::At { tx: 100, op: 5 }.to_string(), "100_5");
    }

    #[test]
    fn offset_ordering() {
        assert!(Offset::Initial < Offset::At { tx: 0, op: 0 });
        assert!(Offset::At { tx: 1, op: 0 } < Offset::At { tx: 2, op: 0 });
        assert!(Offset::At { tx: 1, op: 5 } > Offset::At { tx: 1, op: 3 });
        assert!(Offset::At { tx: 999, op: 0 } < Offset::Now);
    }

    // ── Message deserialization tests ─────────────────────────────────────────

    #[test]
    fn deserialize_up_to_date_control() {
        let json = r#"{"headers":{"control":"up-to-date"}}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_up_to_date());
    }

    #[test]
    fn deserialize_must_refetch_control() {
        let json = r#"{"headers":{"control":"must-refetch"}}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_must_refetch());
    }

    #[test]
    fn deserialize_insert_change() {
        let json = r#"{
            "key": "1",
            "value": {"id": "1", "text": "hello"},
            "headers": {"operation": "insert", "txids": [42]}
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        match msg {
            Message::Change(c) => {
                assert_eq!(c.key, "1");
                assert_eq!(c.headers.operation, Operation::Insert);
                assert_eq!(c.value.get("text").unwrap(), "hello");
            }
            other => panic!("Expected Change, got {:?}", other),
        }
    }

    #[test]
    fn deserialize_delete_change() {
        let json = r#"{
            "key": "5",
            "value": {"id": "5"},
            "headers": {"operation": "delete"}
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            Message::Change(ref c) if c.headers.operation == Operation::Delete
        ));
    }

    #[test]
    fn deserialize_update_with_old_value() {
        let json = r#"{
            "key": "2",
            "value": {"id": "2", "text": "updated"},
            "old_value": {"text": "original"},
            "headers": {"operation": "update"}
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        match msg {
            Message::Change(c) => {
                assert!(c.old_value.is_some());
                let old = c.old_value.unwrap();
                assert_eq!(old.get("text").unwrap(), "original");
            }
            _ => panic!("Expected Change"),
        }
    }

    #[test]
    fn deserialize_message_array() {
        let json = r#"[
            {"key":"1","value":{"id":"1"},"headers":{"operation":"insert"}},
            {"headers":{"control":"up-to-date"}}
        ]"#;
        let msgs: Vec<Message> = serde_json::from_str(json).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(matches!(msgs[0], Message::Change(_)));
        assert!(msgs[1].is_up_to_date());
    }

    #[test]
    fn column_info_deserializes_from_schema_header() {
        let json = r#"{"type":"int4","dimensions":0}"#;
        let ci: ColumnInfo = serde_json::from_str(json).unwrap();
        assert_eq!(ci.pg_type, "int4");
        assert_eq!(ci.dimensions, 0);
    }

    #[test]
    fn schema_deserializes_from_header() {
        let json = r#"{"id":{"type":"int4","dimensions":0},"text":{"type":"text","dimensions":0}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        assert_eq!(schema["id"].pg_type, "int4");
        assert_eq!(schema["text"].pg_type, "text");
    }
}
