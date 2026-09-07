//! G-09: spoken paragraph breaks.
//!
//! "hello new paragraph world" -> "hello\n\nworld", and the Russian
//! equivalent "новый абзац". The two command words are dropped and
//! replaced with a bare blank-line break, with no leftover space on
//! either side.
//!
//! This pass must run dead last in `apply` (see that function's doc
//! comment): every other pass in this module tokenizes by splitting `text`
//! on `' '` and rejoining the same way, so an embedded `\n` glued directly
//! onto its neighboring words (no surrounding space) would otherwise fuse
//! them into one unsplittable token for any pass that ran afterward.
//! Running this last sidesteps that entirely - nothing downstream ever
//! re-tokenizes the result.

use super::split_punct;

/// True for the leading word of either language's "new paragraph" phrase.
fn is_new(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "new" | "новый")
}

/// True for the trailing word of either language's "new paragraph" phrase.
fn is_paragraph(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "paragraph" | "абзац")
}

/// Replaces every spoken "new paragraph"/"новый абзац" with a blank-line
/// break, trimming the space that would otherwise surround it.
pub fn normalize_paragraph_breaks(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if is_new(cores[i]) && cores.get(i + 1).is_some_and(|&c| is_paragraph(c)) {
            out.push("\n\n".to_string());
            i += 2;
            continue;
        }
        out.push(words[i].to_string());
        i += 1;
    }

    out.join(" ")
        .replace(" \n\n ", "\n\n")
        .replace(" \n\n", "\n\n")
        .replace("\n\n ", "\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_new_paragraph() {
        assert_eq!(
            normalize_paragraph_breaks("hello new paragraph world"),
            "hello\n\nworld"
        );
    }

    #[test]
    fn russian_novyi_abzac() {
        assert_eq!(
            normalize_paragraph_breaks("привет новый абзац мир"),
            "привет\n\nмир"
        );
    }

    #[test]
    fn multiple_breaks() {
        assert_eq!(
            normalize_paragraph_breaks("one new paragraph two new paragraph three"),
            "one\n\ntwo\n\nthree"
        );
    }

    #[test]
    fn break_at_the_start_or_end() {
        assert_eq!(
            normalize_paragraph_breaks("new paragraph world"),
            "\n\nworld"
        );
        assert_eq!(
            normalize_paragraph_breaks("hello new paragraph"),
            "hello\n\n"
        );
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(
            normalize_paragraph_breaks("this is a new car"),
            "this is a new car"
        );
        assert_eq!(normalize_paragraph_breaks("hello world"), "hello world");
        assert_eq!(normalize_paragraph_breaks(""), "");
    }
}
