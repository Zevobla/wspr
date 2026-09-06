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
    // The stand-set JSON also carries per-case metadata (голос/темп/
    // длительность/особенность); serde ignores those keys since the bench
    // harness only consumes file/criterion/reference.
}

/// The entire stand set metadata.
#[derive(Debug, Deserialize)]
pub struct StandSet {
    /// Array of test cases. Set-level metadata keys (частота/случаев) in the
    /// JSON are ignored — the harness derives what it needs from `cases`.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_used_fields_and_ignores_extra_metadata() {
        // Real stand-set JSON carries per-case/per-set metadata the harness
        // doesn't consume; serde must ignore those keys and still populate
        // the fields we do use.
        let json = r#"{
            "частота": 16000,
            "случаев": 1,
            "случаи": [{
                "файл": "f-01.wav",
                "критерий": "F-01",
                "эталон": "hi",
                "голос": "Milena",
                "темп": 120,
                "длительность": 3.5,
                "особенность": "clean"
            }]
        }"#;
        let set: StandSet = serde_json::from_str(json).expect("parse");
        assert_eq!(set.cases.len(), 1);
        let c = &set.cases[0];
        assert_eq!(c.file, "f-01.wav");
        assert_eq!(c.criterion, "F-01");
        assert_eq!(c.reference, "hi");
    }
}
