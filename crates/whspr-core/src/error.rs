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

    #[error("diarize backend error: {0}")]
    Diarize(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, WhsprError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whspr_error_asr_variant() {
        let error = WhsprError::Asr("backend failed".to_string());
        assert_eq!(error.to_string(), "asr backend error: backend failed");
    }

    #[test]
    fn whspr_error_refine_variant() {
        let error = WhsprError::Refine("cleanup failed".to_string());
        assert_eq!(error.to_string(), "refine backend error: cleanup failed");
    }

    #[test]
    fn whspr_error_audio_variant() {
        let error = WhsprError::Audio("decode error".to_string());
        assert_eq!(error.to_string(), "audio error: decode error");
    }

    #[test]
    fn whspr_error_other_variant() {
        let error = WhsprError::Other("something went wrong".to_string());
        assert_eq!(error.to_string(), "something went wrong");
    }
}
