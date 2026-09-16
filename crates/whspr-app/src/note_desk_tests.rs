//! Unit tests for `crate::note_desk` (split out to hold the AA-06 line cap).

use super::*;
use whspr_config::Config;

#[test]
fn enter_note_desk_seeds_the_mode() {
    let mut state = State::new(Config::default());
    assert!(state.note_desk.is_none());
    assert!(update(&mut state, &Message::EnterNoteDesk).is_some());
    let nd = state.note_desk.as_ref().expect("note desk seeded");
    assert!(!nd.rows.is_empty());
}

#[test]
fn back_to_dictate_clears_the_mode() {
    let mut state = State::new(Config::default());
    state.note_desk = Some(NoteDeskState::sample());
    assert!(update(&mut state, &Message::BackToDictate).is_some());
    assert!(state.note_desk.is_none());
}

#[test]
fn set_transcript_filter_updates_the_active_desk() {
    let mut state = State::new(Config::default());
    state.note_desk = Some(NoteDeskState::sample());
    let msg = Message::SetTranscriptFilter(TranscriptFilter::Kept);
    assert!(update(&mut state, &msg).is_some());
    assert_eq!(
        state.note_desk.as_ref().expect("desk").filter,
        TranscriptFilter::Kept
    );
}

#[test]
fn update_ignores_unrelated_messages() {
    let mut state = State::new(Config::default());
    assert!(update(&mut state, &Message::ThemeToggled).is_none());
}

#[test]
fn toggle_view_code_flips_the_flag() {
    let mut state = State::new(Config::default());
    state.note_desk = Some(NoteDeskState::sample());
    assert!(!state.note_desk.as_ref().unwrap().view_code);
    assert!(update(&mut state, &Message::NoteDeskToggleViewCode).is_some());
    assert!(state.note_desk.as_ref().unwrap().view_code);
    assert!(update(&mut state, &Message::NoteDeskToggleViewCode).is_some());
    assert!(!state.note_desk.as_ref().unwrap().view_code);
}

#[test]
fn export_done_ok_records_the_saved_path() {
    let mut state = State::new(Config::default());
    state.note_desk = Some(NoteDeskState::sample());
    let path = PathBuf::from("/tmp/note.typ");
    assert!(update(&mut state, &Message::NoteDeskExportTypDone(Ok(Some(path)))).is_some());
    assert_eq!(
        state.note_desk.as_ref().unwrap().export_status.as_deref(),
        Some("Saved Typst to /tmp/note.typ")
    );
}

#[test]
fn export_done_cancel_clears_status() {
    let mut state = State::new(Config::default());
    let mut nd = NoteDeskState::sample();
    nd.export_status = Some("Exporting PDF\u{2026}".to_string());
    state.note_desk = Some(nd);
    assert!(update(&mut state, &Message::NoteDeskExportPdfDone(Ok(None))).is_some());
    assert!(state.note_desk.as_ref().unwrap().export_status.is_none());
}

#[test]
fn export_done_err_records_the_failure() {
    let mut state = State::new(Config::default());
    state.note_desk = Some(NoteDeskState::sample());
    assert!(update(
        &mut state,
        &Message::NoteDeskExportPdfDone(Err("boom".to_string()))
    )
    .is_some());
    assert_eq!(
        state.note_desk.as_ref().unwrap().export_status.as_deref(),
        Some("PDF export failed: boom")
    );
}

#[test]
fn suggested_file_name_sanitizes_and_falls_back() {
    assert_eq!(suggested_file_name("Lecture 7", "typ"), "Lecture 7.typ");
    assert_eq!(suggested_file_name("a/b\\c", "pdf"), "a-b-c.pdf");
    assert_eq!(suggested_file_name("   ", "typ"), "note.typ");
}

#[test]
fn sample_keep_scores_are_in_range() {
    for row in NoteDeskState::sample().rows {
        assert!(row.keep_score <= 3);
    }
}

#[test]
fn sample_has_a_title_and_rows() {
    let nd = NoteDeskState::sample();
    assert!(!nd.title.is_empty());
    assert!(!nd.rows.is_empty());
}

#[test]
fn rank_line_scores_by_length_and_figures() {
    // Very short lines are chatter, kept out of the notes.
    assert_eq!(rank_line("Right, yeah."), (Gutter::Chatter, 0));
    // A short-but-real line is a low-score candidate.
    assert_eq!(
        rank_line("we then cooled the sample"),
        (Gutter::Candidate, 1)
    );
    // A meatier line scores higher but is still a candidate.
    assert_eq!(
        rank_line("we then cooled the sample down and measured it"),
        (Gutter::Candidate, 2)
    );
    // A figure inside a medium line lifts it to kept.
    assert_eq!(
        rank_line("we cooled the sample to 4 kelvin over ten minutes"),
        (Gutter::Kept, 3)
    );
    // A long line is kept on length alone.
    let long = "a b c d e f g h i j k l m n o";
    assert_eq!(rank_line(long), (Gutter::Kept, 3));
}

#[test]
fn from_import_maps_segments_to_candidate_rows() {
    let transcript = whspr_core::Transcript {
        text: "one two".to_string(),
        language: Some("en".to_string()),
        segments: vec![
            whspr_core::TranscriptSegment {
                text: "one two three four five".to_string(),
                start_secs: 5.0,
                end_secs: 8.0,
                speaker: Some("Speaker A".to_string()),
            },
            whspr_core::TranscriptSegment {
                text: "two".to_string(),
                start_secs: 65.0,
                end_secs: 70.0,
                speaker: None,
            },
        ],
    };
    let headings = vec![NoteHeading {
        time_label: "00:00".to_string(),
        title: "Intro".to_string(),
    }];
    let nd = NoteDeskState::from_import("Lecture", headings, &transcript);
    assert_eq!(nd.title, "Lecture");
    assert_eq!(nd.headings.len(), 1);
    assert_eq!(nd.rows.len(), 2);
    assert_eq!(nd.rows[0].time_label, "00:05");
    assert_eq!(nd.rows[0].text, "one two three four five");
    assert_eq!(nd.rows[0].speaker_id.as_deref(), Some("Speaker A"));
    // A 5-word line ranks as a candidate; a bare word ("two") is chatter.
    assert_eq!(nd.rows[0].gutter, Gutter::Candidate);
    assert_eq!(nd.rows[1].time_label, "01:05");
    assert_eq!(nd.rows[1].gutter, Gutter::Chatter);
    assert!(nd.rows[1].speaker_id.is_none());
}

/// A `TranscriptRow` with just the fields the filter reads set.
fn row_with(gutter: Gutter, keep_score: u8) -> TranscriptRow {
    TranscriptRow {
        time_label: "00:00".to_string(),
        text: String::new(),
        speaker_id: None,
        gutter,
        keep_score,
    }
}

#[test]
fn filter_all_keeps_every_row() {
    for gutter in [Gutter::Kept, Gutter::Candidate, Gutter::Chatter] {
        for score in 0..=3 {
            assert!(TranscriptFilter::All.keeps(&row_with(gutter, score)));
        }
    }
}

#[test]
fn filter_kept_keeps_only_kept_rows() {
    assert!(TranscriptFilter::Kept.keeps(&row_with(Gutter::Kept, 0)));
    assert!(!TranscriptFilter::Kept.keeps(&row_with(Gutter::Candidate, 3)));
    assert!(!TranscriptFilter::Kept.keeps(&row_with(Gutter::Chatter, 3)));
}

#[test]
fn filter_key_includes_high_score_candidates() {
    assert!(TranscriptFilter::Key.keeps(&row_with(Gutter::Kept, 0)));
    assert!(TranscriptFilter::Key.keeps(&row_with(Gutter::Candidate, 2)));
    assert!(!TranscriptFilter::Key.keeps(&row_with(Gutter::Candidate, 1)));
}

#[test]
fn filter_default_is_key() {
    assert_eq!(TranscriptFilter::default(), TranscriptFilter::Key);
}

#[test]
fn from_import_without_segments_uses_a_single_text_row() {
    let transcript = whspr_core::Transcript {
        text: "just text".to_string(),
        ..Default::default()
    };
    let nd = NoteDeskState::from_import("Talk", Vec::new(), &transcript);
    assert_eq!(nd.rows.len(), 1);
    assert_eq!(nd.rows[0].time_label, "00:00");
    assert_eq!(nd.rows[0].text, "just text");
    assert!(nd.headings.is_empty());
}
