use serde::Deserialize;
use std::path::Path;
use whspr_core::Result as WhsprResult;
use whspr_core::WhsprError;

/// One test case from the stand set.
#[derive(Debug, Clone, Deserialize)]
pub struct Case {
    /// Audio file name within the аудио/ subdirectory.
    #[serde(rename = "файл")]
    pub file: String,

    /// Criterion code (e.g., "F-01", "AM-03").
    #[serde(rename = "критерий")]
    pub criterion: String,

    /// Ground truth / reference transcription.
    #[serde(rename = "эталон")]
    pub reference: String,

    /// Voice name (e.g., "Milena").
    #[serde(rename = "голос")]
    pub voice: String,

    /// Speech tempo in WPM or similar.
    #[serde(rename = "темп")]
    pub tempo: u32,

    /// Audio duration in seconds.
    #[serde(rename = "длительность")]
    pub duration: f32,

    /// Special feature description (e.g., "чистая запись").
    #[serde(rename = "особенность")]
    pub feature: String,
}

/// The entire stand set metadata.
#[derive(Debug, Deserialize)]
pub struct StandSet {
    /// Audio sample rate (e.g., 16000).
    #[serde(rename = "частота")]
    pub sample_rate: u32,

    /// Total number of test cases.
    #[serde(rename = "случаев")]
    pub count: usize,

    /// Array of test cases.
    #[serde(rename = "случаи")]
    pub cases: Vec<Case>,
}

impl StandSet {
    /// Load the stand set from a JSON file (typically `<stand-set>/эталоны.json`).
    pub fn load(path: impl AsRef<Path>) -> WhsprResult<Self> {
        let path = path.as_ref();
        let data = std::fs::read_to_string(path)
            .map_err(|e| WhsprError::Other(format!("failed to read stand set JSON: {}", e)))?;

        serde_json::from_str(&data)
            .map_err(|e| WhsprError::Other(format!("failed to deserialize stand set: {}", e)))
    }

    /// Extract the criterion prefix (everything before the first hyphen) for grouping.
    pub fn criterion_prefix(criterion: &str) -> String {
        criterion
            .split('-')
            .next()
            .unwrap_or(criterion)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criterion_prefix() {
        assert_eq!(StandSet::criterion_prefix("F-01"), "F");
        assert_eq!(StandSet::criterion_prefix("AM-03"), "AM");
        assert_eq!(StandSet::criterion_prefix("AI-04"), "AI");
        assert_eq!(StandSet::criterion_prefix("single"), "single");
        assert_eq!(StandSet::criterion_prefix("E-06"), "E");
    }
}
