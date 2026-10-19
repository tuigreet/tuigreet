pub mod cli;
pub mod parser;
pub mod schema;

use std::{io, path::PathBuf};

pub use schema::*;
use toml::de::Error as TomlError;

/// Errors that can occur during configuration loading or validation.
#[derive(Debug, flub::Error)]
pub enum ConfigError {
  /// I/O error when reading config files.
  #[error("I/O error: {0}")]
  Io(#[from] io::Error),

  /// TOML parsing error.
  #[error("TOML parse error: {0}")]
  Parse(#[from] TomlError),

  /// TOML parsing error with file context (rendered via codespan).
  #[error("{message}")]
  ParseWithContext { message: String },

  /// General validation error with description.
  #[error("Validation error: {0}")]
  Validation(String),

  /// Two options that cannot be used together.
  #[error("Options '{0}' and '{1}' are mutually exclusive")]
  MutuallyExclusive(String, String),

  /// Option that depends on another option.
  #[error("Dependency error: {0}")]
  Dependency(String),

  /// Invalid range (e.g., `min_uid` >= `max_uid`).
  #[error("Invalid range: {0}")]
  InvalidRange(String),

  /// Duplicate keybindings.
  #[error("Duplicate keybindings detected")]
  DuplicateKeybindings,

  /// Invalid F-key value.
  #[error("Invalid F-key value for '{0}': F{1} (must be F1-F12)")]
  InvalidFKey(String, u8),

  /// Invalid time format string.
  #[error("Invalid time format string")]
  InvalidTimeFormat,

  /// Session wrapper executable not found.
  #[error("Session wrapper executable not found: {0}")]
  WrapperExecutableNotFound(PathBuf),
}
