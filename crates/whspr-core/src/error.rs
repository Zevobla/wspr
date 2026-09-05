use thiserror::Error;

/// The single error type shared across every whspr crate. Backend and
/// integration crates should wrap their own errors into a variant here (or
/// use `Other`) rather than inventing parallel error types.
#[derive(Debug, Error)]
pub enum WhsprError {
    #[error("asr backend error: {0}")]
    Asr(String),

    #[error("refine backend error: {0}")]
    Refine(String),

    #[error("audio error: {0}")]
    Audio(String),

    #[error("injection error: {0}")]
    Inject(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, WhsprError>;
