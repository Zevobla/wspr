//! Pure mapping between `whspr_config`'s backend-choice enums and the string
//! labels shown in the Hub's `pick_list` widgets.
//!
//! `AsrChoice`/`RefineChoice` live in `whspr-config` and can't implement
//! `Display` for us to hand straight to `pick_list` (orphan rules -- neither
//! the trait nor the type is ours), so we keep small label tables here
//! instead and translate both ways.

use whspr_config::{AsrChoice, RefineChoice, SpeakerEmbeddingChoice};

/// Labels shown in the ASR backend `pick_list`, in display order.
pub const ASR_LABELS: [&str; 3] = ["whisper-local", "openai", "deepgram"];

/// Labels shown in the refiner `pick_list`, in display order.
pub const REFINE_LABELS: [&str; 4] = ["noop", "openai", "anthropic", "llama-local"];

/// Labels shown in the speaker-embedding-model `pick_list`, in display
/// order.
pub const EMBEDDING_LABELS: [&str; 2] = ["cam-plus-plus", "eres2net"];

/// Labels shown in the language-override `pick_list`, in display order.
/// "auto" means no override (`Config.language == None`); every other entry
/// is a BCP47 code passed straight through. A curated subset of whisper's
/// supported languages, not an exhaustive list -- easy to extend later.
pub const LANGUAGE_LABELS: [&str; 11] = [
    "auto", "en", "ru", "es", "fr", "de", "it", "pt", "ja", "ko", "zh",
];

/// The label a `pick_list` should show as selected for the given choice.
pub fn asr_label(choice: AsrChoice) -> &'static str {
    match choice {
        AsrChoice::WhisperLocal => "whisper-local",
        AsrChoice::OpenAi => "openai",
        AsrChoice::Deepgram => "deepgram",
        // Test-only backend, deliberately not in ASR_LABELS (never offered
        // in the Hub's pick_list) - matched here only to keep this
        // exhaustive.
        AsrChoice::Mock => "mock",
    }
}

/// The label a `pick_list` should show as selected for the given choice.
pub fn refine_label(choice: RefineChoice) -> &'static str {
    match choice {
        RefineChoice::Noop => "noop",
        RefineChoice::OpenAi => "openai",
        RefineChoice::Anthropic => "anthropic",
        RefineChoice::LlamaLocal => "llama-local",
    }
}

/// Recovers the `AsrChoice` for a label previously produced by `asr_label`.
/// Falls back to the default choice for any unrecognized label rather than
/// panicking, since this only ever runs on labels we generated ourselves.
pub fn asr_from_label(label: &str) -> AsrChoice {
    match label {
        "openai" => AsrChoice::OpenAi,
        "deepgram" => AsrChoice::Deepgram,
        _ => AsrChoice::WhisperLocal,
    }
}

/// Recovers the `RefineChoice` for a label previously produced by
/// `refine_label`. Falls back to the default choice for any unrecognized
/// label rather than panicking.
pub fn refine_from_label(label: &str) -> RefineChoice {
    match label {
        "openai" => RefineChoice::OpenAi,
        "anthropic" => RefineChoice::Anthropic,
        "llama-local" => RefineChoice::LlamaLocal,
        _ => RefineChoice::Noop,
    }
}

/// The label a `pick_list` should show as selected for the given choice.
pub fn embedding_label(choice: SpeakerEmbeddingChoice) -> &'static str {
    match choice {
        SpeakerEmbeddingChoice::CamPlusPlus => "cam-plus-plus",
        SpeakerEmbeddingChoice::Eres2Net => "eres2net",
    }
}

/// Recovers the `SpeakerEmbeddingChoice` for a label previously produced by
/// `embedding_label`. Falls back to the default choice for any unrecognized
/// label rather than panicking.
pub fn embedding_from_label(label: &str) -> SpeakerEmbeddingChoice {
    match label {
        "eres2net" => SpeakerEmbeddingChoice::Eres2Net,
        _ => SpeakerEmbeddingChoice::CamPlusPlus,
    }
}

/// The label the language `pick_list` should show as selected for
/// `Config.language`: "auto" for `None`, the code itself if it's one of
/// `LANGUAGE_LABELS`, or "auto" for anything else (a language the user
/// (or an older config file) set that isn't in this curated list -- rather
/// than crash or silently invent a new pick_list entry, the picker just
/// shows no confident match).
pub fn language_label(language: &Option<String>) -> &'static str {
    match language {
        None => "auto",
        Some(code) => LANGUAGE_LABELS
            .iter()
            .find(|&&label| label == code)
            .copied()
            .unwrap_or("auto"),
    }
}

/// Recovers the `Config.language` value for a label previously produced by
/// `language_label` (or picked directly from `LANGUAGE_LABELS`): `None` for
/// "auto", `Some(label)` otherwise.
pub fn language_from_label(label: &str) -> Option<String> {
    if label == "auto" {
        None
    } else {
        Some(label.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_label_roundtrips_through_from_label() {
        for choice in [
            AsrChoice::WhisperLocal,
            AsrChoice::OpenAi,
            AsrChoice::Deepgram,
        ] {
            assert_eq!(asr_from_label(asr_label(choice)), choice);
        }
    }

    #[test]
    fn refine_label_roundtrips_through_from_label() {
        for choice in [
            RefineChoice::Noop,
            RefineChoice::OpenAi,
            RefineChoice::Anthropic,
            RefineChoice::LlamaLocal,
        ] {
            assert_eq!(refine_from_label(refine_label(choice)), choice);
        }
    }

    #[test]
    fn asr_from_label_falls_back_to_default_for_unknown_label() {
        assert_eq!(
            asr_from_label("not-a-real-backend"),
            AsrChoice::WhisperLocal
        );
    }

    #[test]
    fn refine_from_label_falls_back_to_default_for_unknown_label() {
        assert_eq!(refine_from_label("not-a-real-backend"), RefineChoice::Noop);
    }

    #[test]
    fn embedding_label_roundtrips_through_from_label() {
        for choice in [
            SpeakerEmbeddingChoice::CamPlusPlus,
            SpeakerEmbeddingChoice::Eres2Net,
        ] {
            assert_eq!(embedding_from_label(embedding_label(choice)), choice);
        }
    }

    #[test]
    fn embedding_from_label_falls_back_to_default_for_unknown_label() {
        assert_eq!(
            embedding_from_label("not-a-real-embedding-model"),
            SpeakerEmbeddingChoice::CamPlusPlus
        );
    }

    #[test]
    fn language_label_roundtrips_through_from_label() {
        for &label in LANGUAGE_LABELS.iter() {
            let language = language_from_label(label);
            assert_eq!(language_label(&language), label);
        }
    }

    #[test]
    fn language_label_none_is_auto() {
        assert_eq!(language_label(&None), "auto");
    }

    #[test]
    fn language_label_unknown_code_falls_back_to_auto() {
        assert_eq!(
            language_label(&Some("xx-not-a-real-code".to_string())),
            "auto"
        );
    }

    #[test]
    fn language_from_label_auto_is_none() {
        assert_eq!(language_from_label("auto"), None);
    }
}
