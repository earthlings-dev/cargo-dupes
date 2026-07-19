//! Typed error model of the analysis pipeline.

use std::path::PathBuf;

/// Errors the analysis pipeline can surface to its callers.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Reading a source file failed.
  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),

  /// The scan found nothing analyzable under the root path.
  #[error("No source files found in {0}")]
  NoSourceFiles(PathBuf),

  /// Any other pipeline failure, carried as its rendered message.
  #[error("{0}")]
  Other(String),
}

/// Crate-wide result alias over [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
