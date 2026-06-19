//! # electric-client
//!
//! Async Rust client for the [Electric SQL](https://electric-sql.com/) sync service.
//!
//! Syncs Postgres tables to your Rust application in real time using Electric's HTTP
//! shape-streaming protocol. Provides two levels of API:
//!
//! - [`ShapeStream`]: low-level async iterator of message batches (change events +
//!   control messages). Suitable for pipeline/streaming use cases.
//! - [`Shape`]: higher-level materialized view that maintains an in-memory map of rows,
//!   applies inserts/updates/deletes, and notifies subscribers on each up-to-date
//!   batch.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use electric_client::{Shape, ShapeStream, ShapeStreamOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let stream = ShapeStream::new(ShapeStreamOptions {
//!         url: "http://localhost:3000/v1/shape".to_string(),
//!         table: "todos".to_string(),
//!         subscribe: false, // fetch once and stop
//!         ..Default::default()
//!     })?;
//!
//!     let shape = Shape::new(stream);
//!     let rows = shape.rows().await;
//!     println!("Synced {} rows", rows.len());
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod constants;
pub mod error;
pub mod fetch;
pub mod parser;
pub mod shape;
pub mod types;

// Re-export the primary public API
pub use client::{ShapeEvent, ShapeStream, ShapeStreamOptions};
pub use error::ElectricError;
pub use fetch::{BackoffOptions, Fetcher, ReqwestFetcher};
pub use parser::Parser;
pub use shape::Shape;
pub use types::{
    ChangeHeaders, ChangeMessage, ColumnInfo, ControlKind, ControlMessage, Message, Offset,
    Operation, Replica, Row, Schema,
};
