//! `protocol-dumper` library API.
//!
//! The binary in `main.rs` is a thin CLI over [`inventory::build`] and
//! [`connection::build`]. Integration tests use the same entry points.

pub mod connection;
pub mod inventory;
pub mod java;

use std::path::PathBuf;

/// Errors returned by the dumper.
#[derive(Debug, thiserror::Error)]
pub enum DumperError {
    /// A filesystem operation failed.
    #[error("{path}: {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A Java source did not match the generated grammar.
    #[error("{path}:{line}: {message}")]
    Parse {
        /// Source file.
        path: PathBuf,
        /// 1-based line number, or 0 when the whole file is at fault.
        line: usize,
        /// What went wrong.
        message: String,
    },
    /// JSON serialization failed.
    #[error("serialize: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result alias used throughout the tool.
pub type Result<T> = std::result::Result<T, DumperError>;
