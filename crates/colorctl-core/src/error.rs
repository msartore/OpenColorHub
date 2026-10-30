//! Error type shared by the core, the CLI and the Tauri commands.

use std::path::PathBuf;

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The selected backend is not available on this machine/OS.
    #[error("{0}")]
    Unavailable(String),

    /// A backend rejected an operation (bad register value, write failed, ...).
    #[error("{0}")]
    Backend(String),

    /// Caller supplied invalid input.
    #[error("{0}")]
    Invalid(#[from] CurveError),

    #[error("settings file {path} is malformed: {source}")]
    Settings {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::Unavailable(msg.into())
    }

    pub fn backend(msg: impl Into<String>) -> Self {
        Self::Backend(msg.into())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(CurveError::Message(msg.into()))
    }
}

/// Errors produced while validating a fan curve.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurveError {
    #[error("a fan curve needs exactly {expected} points, got {got}")]
    WrongCount { expected: usize, got: usize },

    #[error("temperature points must be strictly increasing: {temp}°C follows {previous}°C")]
    NotAscending { temp: u8, previous: u8 },

    #[error("speed {percent}% is out of range (0-100)")]
    PercentOutOfRange { percent: u8 },

    #[error("temperature {temp}°C is out of range (0-120)")]
    TempOutOfRange { temp: u8 },

    #[error("{0}")]
    Message(String),
}
