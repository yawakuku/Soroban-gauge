//! Error type shared by the core crate, with stable CLI exit codes.

/// Errors produced while loading config, running scenarios or handling baselines.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Bad configuration or usage (exit code 2).
    #[error("{0}")]
    Config(String),
    /// A scenario step failed, panicked or trapped unexpectedly (exit code 3).
    #[error("{0}")]
    Scenario(String),
    /// Baseline and current environments differ (exit code 4).
    #[error("{0}")]
    VersionMismatch(String),
    /// I/O or parse failure (exit code 5).
    #[error("{0}")]
    Io(String),
}

impl CoreError {
    /// The process exit code documented for this error class.
    pub fn exit_code(&self) -> i32 {
        match self {
            CoreError::Config(_) => 2,
            CoreError::Scenario(_) => 3,
            CoreError::VersionMismatch(_) => 4,
            CoreError::Io(_) => 5,
        }
    }

    /// Builds an I/O error that names the path involved.
    pub fn io(path: &std::path::Path, e: impl std::fmt::Display) -> Self {
        CoreError::Io(format!("{}: {e}", path.display()))
    }
}
