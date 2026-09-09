//! The three note templates.
//!
//! Every template produces the same locked document shape — an import of the
//! `@local/whspr:0.2.0` package followed by `#show: lecture.with(..)` and a
//! stream of `#point(..)` lines (see [`crate::generate::generate_notes_typ`]).
//! A template contributes a short *prelude* of top-level `#set` rules that
//! runs before the show-rule, tuning the page for its use case. Each prelude
//! only uses knobs the `lecture` show-rule itself leaves alone (column count,
//! heading numbering, paragraph leading), so the two compose cleanly.

/// Which note layout [`generate_notes_typ`](crate::generate::generate_notes_typ)
/// should emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Flowing single-column lecture notes.
    Lecture,
    /// A compact two-column sheet for review/revision.
    StudySheet,
    /// Numbered meeting minutes.
    Minutes,
}

/// Lecture notes: a single column with roomy leading.
pub(crate) const LECTURE: &str = "#set par(leading: 0.7em)\n";

/// Study sheet: two compact columns for revision.
pub(crate) const STUDY_SHEET: &str = "#set page(columns: 2)\n#set par(leading: 0.55em)\n";

/// Meeting minutes: numbered sections, tighter spacing.
pub(crate) const MINUTES: &str = "#set heading(numbering: \"1.\")\n#set par(leading: 0.6em)\n";

impl Template {
    /// The template's top-level `#set`-rule prelude, inserted between the
    /// package import and the `#show: lecture.with(..)` line.
    pub(crate) fn prelude(self) -> &'static str {
        match self {
            Template::Lecture => LECTURE,
            Template::StudySheet => STUDY_SHEET,
            Template::Minutes => MINUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_prelude_is_a_nonempty_set_block() {
        for template in [Template::Lecture, Template::StudySheet, Template::Minutes] {
            let prelude = template.prelude();
            assert!(!prelude.is_empty());
            assert!(prelude.contains("#set"));
        }
    }

    #[test]
    fn study_sheet_is_two_columns() {
        assert!(Template::StudySheet.prelude().contains("columns: 2"));
    }

    #[test]
    fn minutes_numbers_headings() {
        assert!(Template::Minutes.prelude().contains("numbering"));
    }
}
