//! Unit tests for `crate` (config), split out to hold the AA-06 line cap.

use super::*;
use std::io::Write;

#[test]
fn load_returns_defaults() {
    let cfg = load();
    assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
    assert_eq!(cfg.refine, RefineChoice::Noop);
    assert_eq!(cfg.language, None);
}

#[test]
fn load_from_none_returns_defaults() {
    let cfg = load_from(None);
    assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
    assert_eq!(cfg.refine, RefineChoice::Noop);
    assert_eq!(cfg.language, None);
}

#[test]
fn load_from_toml_file_overrides() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "asr = \"open-ai\"").expect("failed to write asr");
    writeln!(file, "refine = \"open-ai\"").expect("failed to write refine");
    writeln!(file, "language = \"es\"").expect("failed to write language");
    drop(file); // ensure file is closed

    let cfg = load_from(Some(temp_dir.path()));
    assert_eq!(cfg.asr, AsrChoice::OpenAi);
    assert_eq!(cfg.refine, RefineChoice::OpenAi);
    assert_eq!(cfg.language, Some("es".to_string()));
}

#[test]
fn load_from_partial_toml_merges_with_defaults() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "asr = \"deepgram\"").expect("failed to write asr");
    drop(file);

    let cfg = load_from(Some(temp_dir.path()));
    assert_eq!(cfg.asr, AsrChoice::Deepgram);
    assert_eq!(cfg.refine, RefineChoice::Noop); // default
    assert_eq!(cfg.language, None); // default
}

#[test]
fn asr_choice_from_str_accepts_mock() {
    assert_eq!(AsrChoice::from_str("mock"), Ok(AsrChoice::Mock));
    assert_eq!(AsrChoice::from_str("MOCK"), Ok(AsrChoice::Mock));
}

#[test]
fn asr_choice_from_str_accepts_apple_speech() {
    for s in ["apple-speech", "apple_speech", "AppleSpeech", "apple"] {
        assert_eq!(
            AsrChoice::from_str(s),
            Ok(AsrChoice::AppleSpeech),
            "expected {s:?} to parse as AppleSpeech"
        );
    }
}

#[test]
fn refine_choice_from_str_accepts_apple_foundation() {
    for s in ["apple-foundation", "apple_foundation", "AppleFoundation"] {
        assert_eq!(
            RefineChoice::from_str(s),
            Ok(RefineChoice::AppleFoundation),
            "expected {s:?} to parse as AppleFoundation"
        );
    }
}

#[test]
fn apple_speech_choice_round_trips_through_toml() {
    let cfg = Config {
        asr: AsrChoice::AppleSpeech,
        ..Default::default()
    };
    let toml_string = toml::to_string_pretty(&cfg).expect("serialize");
    assert!(
        toml_string.contains("asr = \"apple-speech\""),
        "AppleSpeech should serialize kebab-case: {toml_string}"
    );
    let round_tripped: Config = toml::from_str(&toml_string).expect("deserialize");
    assert_eq!(round_tripped.asr, AsrChoice::AppleSpeech);
}

#[test]
fn load_from_missing_file_returns_defaults() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let cfg = load_from(Some(temp_dir.path()));
    assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
    assert_eq!(cfg.refine, RefineChoice::Noop);
    assert_eq!(cfg.language, None);
}

#[test]
fn load_from_creates_config_file_on_first_run() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    assert!(!config_path.exists(), "precondition: no file yet");

    let _first_run = load_from(Some(temp_dir.path()));
    assert!(
        config_path.is_file(),
        "load_from() should have written a default config.toml"
    );

    // A second load reads back exactly what got written, with no
    // further changes.
    let second_run = load_from(Some(temp_dir.path()));
    assert_eq!(second_run.asr, AsrChoice::WhisperLocal);
    assert_eq!(second_run.refine, RefineChoice::Noop);
    assert_eq!(second_run.language, None);
}

#[test]
fn load_from_ignores_environment_variables() {
    // Regression test: env vars must never override the config file or
    // defaults, even for names the old design specifically read.
    std::env::set_var("WHSPR_ASR", "deepgram");
    std::env::set_var("WHSPR_REFINE", "open-ai");
    std::env::set_var("WHSPR_LANGUAGE", "fr");

    let cfg = load_from(None);

    std::env::remove_var("WHSPR_ASR");
    std::env::remove_var("WHSPR_REFINE");
    std::env::remove_var("WHSPR_LANGUAGE");

    assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
    assert_eq!(cfg.refine, RefineChoice::Noop);
    assert_eq!(cfg.language, None);
}

#[test]
fn api_key_for_reads_from_config_file() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "[api_keys]").expect("failed to write api_keys header");
    // Deliberately not shaped like a real provider key (no "sk-" prefix)
    // so this fixture doesn't trip secret scanners looking at the repo.
    writeln!(file, "openai = \"test-openai-key\"").expect("failed to write openai key");
    drop(file);

    let cfg = load_from(Some(temp_dir.path()));
    assert_eq!(
        api_key_for(&cfg, "openai"),
        Some("test-openai-key".to_string())
    );
    assert_eq!(api_key_for(&cfg, "anthropic"), None);
}

#[test]
fn api_key_for_ignores_environment_variables() {
    std::env::set_var("WHSPR_OPENAI_API_KEY", "should-be-ignored");
    let key = api_key_for(&Config::default(), "openai");
    std::env::remove_var("WHSPR_OPENAI_API_KEY");
    assert_eq!(key, None);
}

#[test]
fn save_then_load_round_trips() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let mut cfg = Config {
        asr: AsrChoice::OpenAi,
        ..Default::default()
    };
    cfg.speaker.similarity_threshold = 0.8;
    cfg.save(temp_dir.path()).expect("save should succeed");

    let loaded = load_from(Some(temp_dir.path()));
    assert_eq!(loaded.asr, AsrChoice::OpenAi);
    assert_eq!(loaded.speaker.similarity_threshold, 0.8);
}

#[test]
fn speaker_embedding_choice_defaults_to_cam_plus_plus() {
    assert_eq!(
        SpeakerSettings::default().embedding_model,
        SpeakerEmbeddingChoice::CamPlusPlus
    );
}

#[test]
fn speaker_embedding_choice_from_str_accepts_known_spellings() {
    for s in ["cam-plus-plus", "CamPlusPlus", "campplus", "cam++"] {
        assert_eq!(
            SpeakerEmbeddingChoice::from_str(s),
            Ok(SpeakerEmbeddingChoice::CamPlusPlus),
            "expected {s:?} to parse as CamPlusPlus"
        );
    }
    for s in ["eres2net", "ERes2Net", "eres2-net"] {
        assert_eq!(
            SpeakerEmbeddingChoice::from_str(s),
            Ok(SpeakerEmbeddingChoice::Eres2Net),
            "expected {s:?} to parse as Eres2Net"
        );
    }
    assert!(SpeakerEmbeddingChoice::from_str("nonexistent-model").is_err());
}

#[test]
fn speaker_embedding_choice_maps_to_expected_filenames() {
    assert_eq!(
        SpeakerEmbeddingChoice::CamPlusPlus.filename(),
        "embedding-campplus.onnx"
    );
    assert_eq!(
        SpeakerEmbeddingChoice::Eres2Net.filename(),
        "embedding-eres2net.onnx"
    );
}

#[test]
fn speaker_settings_embedding_model_round_trips_through_toml() {
    let mut cfg = Config::default();
    cfg.speaker.embedding_model = SpeakerEmbeddingChoice::Eres2Net;

    let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
    let round_tripped: Config =
        toml::from_str(&toml_string).expect("failed to deserialize config");

    assert_eq!(
        round_tripped.speaker.embedding_model,
        SpeakerEmbeddingChoice::Eres2Net
    );
}

#[test]
fn config_reload_reads_modified_file() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "asr = \"open-ai\"").expect("failed to write asr");
    drop(file);

    // First load
    let cfg1 = config_reload(temp_dir.path()).expect("first reload should succeed");
    assert_eq!(cfg1.asr, AsrChoice::OpenAi);

    // Modify the file
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "asr = \"deepgram\"").expect("failed to write asr");
    drop(file);

    // Reload picks up the change
    let cfg2 = config_reload(temp_dir.path()).expect("second reload should succeed");
    assert_eq!(cfg2.asr, AsrChoice::Deepgram);
}

#[test]
fn config_reload_returns_error_on_missing_file() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let result = config_reload(temp_dir.path());
    assert!(
        result.is_err(),
        "config_reload should return error on missing file"
    );
}

#[test]
fn config_reload_returns_error_on_invalid_toml() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "asr = invalid syntax").expect("failed to write invalid toml");
    drop(file);

    let result = config_reload(temp_dir.path());
    assert!(
        result.is_err(),
        "config_reload should return error on invalid TOML"
    );
}
