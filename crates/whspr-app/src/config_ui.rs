//! Pure mapping between `whspr_config`'s backend-choice enums and the string
//! labels shown in the Hub's `pick_list` widgets.
//!
//! `AsrChoice`/`RefineChoice` live in `whspr-config` and can't implement
//! `Display` for us to hand straight to `pick_list` (orphan rules -- neither
//! the trait nor the type is ours), so we keep small label tables here
//! instead and translate both ways.

use whspr_config::{AsrChoice, RefineChoice};

/// Labels shown in the ASR backend `pick_list`, in display order.
pub const ASR_LABELS: [&str; 3] = ["whisper-local", "openai", "deepgram"];

/// Labels shown in the refiner `pick_list`, in display order.
pub const REFINE_LABELS: [&str; 4] = ["noop", "openai", "anthropic", "llama-local"];

/// The label a `pick_list` should show as selected for the given choice.
pub fn asr_label(choice: AsrChoice) -> &'static str {
    match choice {
        AsrChoice::WhisperLocal => "whisper-local",
        AsrChoice::OpenAi => "openai",
        AsrChoice::Deepgram => "deepgram",
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
}
