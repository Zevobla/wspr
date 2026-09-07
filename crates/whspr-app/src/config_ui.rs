//! Pure mapping between `whspr_config`'s choice enums and the string labels
//! shown in the Hub's `pick_list` widgets (language, speaker-embedding model,
//! number format).
//!
//! These enums live in `whspr-config` and can't implement `Display` for us to
//! hand straight to `pick_list` (orphan rules -- neither the trait nor the
//! type is ours), so we keep small label tables here instead and translate
//! both ways. The ASR/refiner backends are *not* here: they're picked in the
//! Models tab via the unified `crate::model_menu` selectors, whose `Display`
//! types they own.

use whspr_config::{NumberFormat, SpeakerEmbeddingChoice};

/// Labels shown in the speaker-embedding-model `pick_list`, in display
/// order.
pub const EMBEDDING_LABELS: [&str; 2] = ["cam-plus-plus", "eres2net"];

/// Labels shown in the language-override `pick_list`, in display order.
/// "auto" means full multilingual auto-detect across every language whisper
/// supports (`Config.language == None`, via `whspr_config::effective_language`
/// -- not limited to any particular pair); every other entry is a BCP47 code
/// passed straight through as a *fixed* language. A broader, curated subset
/// of whisper's ~99 supported languages, not an exhaustive list -- easy to
/// extend later.
pub const LANGUAGE_LABELS: [&str; 29] = [
    "auto", "en", "ru", "es", "fr", "de", "it", "pt", "ja", "ko", "zh", "nl", "pl", "tr", "uk",
    "ar", "hi", "sv", "cs", "fi", "da", "no", "el", "he", "ro", "hu", "vi", "th", "id",
];

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

/// Labels shown in the normalize screen's number-rendering `pick_list`, in
/// display order.
pub const NUMBER_FORMAT_LABELS: [&str; 2] = ["digits", "words"];

/// The label a `pick_list` should show as selected for the given choice.
pub fn number_format_label(choice: NumberFormat) -> &'static str {
    match choice {
        NumberFormat::Digits => "digits",
        NumberFormat::Words => "words",
    }
}

/// Recovers the `NumberFormat` for a label previously produced by
/// `number_format_label`. Falls back to the default choice for any
/// unrecognized label rather than panicking.
pub fn number_format_from_label(label: &str) -> NumberFormat {
    match label {
        "words" => NumberFormat::Words,
        _ => NumberFormat::Digits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn number_format_label_roundtrips_through_from_label() {
        for choice in [NumberFormat::Digits, NumberFormat::Words] {
            assert_eq!(
                number_format_from_label(number_format_label(choice)),
                choice
            );
        }
    }

    #[test]
    fn number_format_from_label_falls_back_to_digits_for_unknown_label() {
        assert_eq!(
            number_format_from_label("not-a-real-format"),
            NumberFormat::Digits
        );
    }
}
