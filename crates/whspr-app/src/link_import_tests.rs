//! Unit tests for `crate::link_import` (split out to hold the AA-06 line cap).

use super::*;
use whspr_config::Config;
use whspr_import::{Chapter, Lang, MediaInfo};

fn open_state() -> State {
    let mut state = State::new(Config::default());
    assert!(update(&mut state, &Message::LinkImportOpen).is_some());
    state
}

fn sample_media(human: bool) -> MediaInfo {
    MediaInfo {
        title: "Lecture".to_string(),
        chapters: vec![
            Chapter {
                title: "One".to_string(),
                start_secs: 0.0,
                end_secs: 60.0,
            },
            Chapter {
                title: "Two".to_string(),
                start_secs: 60.0,
                end_secs: 120.0,
            },
        ],
        human_captions: if human {
            vec![Lang {
                code: "en".to_string(),
                name: Some("English".to_string()),
                url: Some("https://example.com/en.vtt".to_string()),
                ext: Some("vtt".to_string()),
            }]
        } else {
            Vec::new()
        },
        auto_captions: vec![Lang {
            code: "de".to_string(),
            name: None,
            url: Some("https://example.com/de.json3".to_string()),
            ext: Some("json3".to_string()),
        }],
        ..MediaInfo::default()
    }
}

#[test]
fn open_seeds_a_dialog_and_cancel_clears_it() {
    let mut state = open_state();
    assert!(state.link_import.is_some());
    assert!(update(&mut state, &Message::LinkImportCancel).is_some());
    assert!(state.link_import.is_none());
}

#[test]
fn url_edits_are_stored() {
    let mut state = open_state();
    assert!(update(&mut state, &Message::LinkImportUrl("https://x".to_string())).is_some());
    assert_eq!(state.link_import.as_ref().unwrap().url, "https://x");
}

#[test]
fn resolved_seeds_all_chapters_included() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
    )
    .is_some());
    let li = state.link_import.as_ref().unwrap();
    assert_eq!(li.chapters_included, vec![true, true]);
    assert!(li.media.is_some());
    assert!(!li.resolving);
}

#[test]
fn resolved_defaults_to_captions_when_a_human_track_exists() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
    )
    .is_some());
    let li = state.link_import.as_ref().unwrap();
    assert!(li.use_captions);
    assert_eq!(li.caption_lang.as_deref(), Some("en"));
}

#[test]
fn resolved_defaults_to_the_auto_caption_when_no_human_track() {
    // With only auto captions, still default to the (instant, direct-URL)
    // caption path — it imports even when the audio stream is bot-walled.
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(sample_media(false))))
    )
    .is_some());
    let li = state.link_import.as_ref().unwrap();
    assert!(li.use_captions);
    assert_eq!(li.caption_lang.as_deref(), Some("de"));
}

#[test]
fn resolved_defaults_to_transcribe_without_any_captions() {
    let mut state = open_state();
    let mut media = sample_media(false);
    media.auto_captions.clear();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(media)))
    )
    .is_some());
    let li = state.link_import.as_ref().unwrap();
    assert!(!li.use_captions);
    assert_eq!(li.caption_lang, None);
}

#[test]
fn resolved_error_is_recorded() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Err("boom".to_string()))
    )
    .is_some());
    let li = state.link_import.as_ref().unwrap();
    assert_eq!(li.error.as_deref(), Some("boom"));
    assert!(li.media.is_none());
}

#[test]
fn toggle_chapter_flips_one_flag() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
    )
    .is_some());
    assert!(update(&mut state, &Message::LinkImportToggleChapter(0)).is_some());
    assert_eq!(
        state.link_import.as_ref().unwrap().chapters_included,
        vec![false, true]
    );
}

#[test]
fn confirm_starts_import_and_keeps_the_dialog_open() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
    )
    .is_some());
    assert!(update(&mut state, &Message::LinkImportUrl("https://x".to_string())).is_some());
    assert!(update(&mut state, &Message::LinkImportConfirm).is_some());
    // The dialog stays open while the async import runs; the returned task
    // is never polled in a unit test (no iced runtime), so no network runs.
    assert!(state.link_import.is_some());
    assert!(state.transcribe_status.is_some());
}

#[test]
fn parse_clip_range_needs_both_bounds() {
    assert!(parse_clip_range("", "").is_none());
    assert!(parse_clip_range("1:00", "").is_none());
    let clip = parse_clip_range("1:00", "1:30").expect("both bounds parse");
    assert_eq!(clip.start_secs, 60.0);
    assert_eq!(clip.end_secs, 90.0);
}

#[test]
fn parse_clip_range_rejects_empty_or_inverted_ranges() {
    assert!(parse_clip_range("1:30", "1:00").is_none());
    assert!(parse_clip_range("1:00", "1:00").is_none());
}

#[test]
fn update_ignores_unrelated_messages() {
    let mut state = open_state();
    assert!(update(&mut state, &Message::ThemeToggled).is_none());
}

#[test]
fn imported_ok_enters_the_note_desk() {
    let mut state = open_state();
    let transcript = whspr_core::Transcript {
        text: "hello world".to_string(),
        ..Default::default()
    };
    let payload = Box::new(("Lecture".to_string(), Vec::new(), transcript));
    assert!(update(&mut state, &Message::LinkImportImported(Ok(payload))).is_some());
    assert!(state.link_import.is_none());
    assert!(state.note_desk.is_some());
    assert!(state.transcribe_status.is_none());
}

#[test]
fn imported_err_keeps_the_dialog_open_with_the_error() {
    let mut state = open_state();
    assert!(update(
        &mut state,
        &Message::LinkImportImported(Err("boom".to_string()))
    )
    .is_some());
    let li = state
        .link_import
        .as_ref()
        .expect("dialog stays open on error");
    assert_eq!(li.error.as_deref(), Some("boom"));
    assert!(state.note_desk.is_none());
}
