//! End-to-end: every template must generate source that actually compiles to
//! a preview and a PDF, using only the `typst-assets` base fonts (no network,
//! no caller fonts).

use whspr_typst::{
    compile_preview_svg, export_pdf, generate_notes_typ, NotePoint, NotesMeta, Template,
};

fn sample_meta() -> NotesMeta {
    NotesMeta {
        title: "Introduction to Photosynthesis".to_string(),
        speaker: "Dr. Rowan Vale".to_string(),
        source: "Biology 210, Lecture 3".to_string(),
        date: "2026-09-09".to_string(),
    }
}

fn sample_points() -> Vec<NotePoint> {
    vec![
        NotePoint {
            t: "09:01".to_string(),
            text: "Photosynthesis converts light energy into chemical energy.".to_string(),
            heading: Some("Overview".to_string()),
        },
        NotePoint {
            t: "09:07".to_string(),
            text: "The light-dependent reactions occur in the thylakoid membrane.".to_string(),
            heading: Some("Light reactions".to_string()),
        },
        NotePoint {
            t: "09:15".to_string(),
            text: "The Calvin cycle fixes carbon dioxide into glucose.".to_string(),
            heading: None,
        },
    ]
}

#[test]
fn all_three_templates_compile_to_nonempty_svg_pages() {
    for template in [Template::Lecture, Template::StudySheet, Template::Minutes] {
        let source = generate_notes_typ(sample_meta(), &sample_points(), template);
        let pages = compile_preview_svg(&source, &[])
            .unwrap_or_else(|e| panic!("template {template:?} failed to compile: {e}"));
        assert!(!pages.is_empty(), "template {template:?} produced no pages");
        for (i, page) in pages.iter().enumerate() {
            assert!(!page.is_empty(), "template {template:?} page {i} SVG was empty");
            let svg = std::str::from_utf8(page).expect("svg is utf-8");
            assert!(svg.contains("<svg"), "template {template:?} page {i} is not SVG");
        }
    }
}

#[test]
fn lecture_template_exports_a_pdf() {
    let source = generate_notes_typ(sample_meta(), &sample_points(), Template::Lecture);
    let pdf = export_pdf(&source, &[]).expect("pdf export");
    assert!(pdf.starts_with(b"%PDF"), "exported bytes are not a PDF");
}
