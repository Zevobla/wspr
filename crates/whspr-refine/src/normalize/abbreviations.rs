//! F-18: known acronyms uppercased.
//!
//! Speech-to-text often lowercases initialisms ("nasa", "сша"). This pass
//! uppercases any word whose bare core (case-insensitively) is on a curated
//! acronym list, leaving its surrounding punctuation in place. The list is
//! deliberately limited to initialisms that are not also ordinary words, so
//! everyday text ("it", "us", "led") is never clobbered.

use super::split_punct;

/// Curated English + Russian acronyms, all lowercase for case-insensitive
/// matching. Ambiguous-with-a-common-word entries are intentionally omitted.
const ACRONYMS: &[&str] = &[
    // English initialisms
    "usa", "uk", "eu", "un", "nasa", "fbi", "cia", "nato", "gps", "api", "url", "uri", "html",
    "css", "http", "https", "sql", "json", "xml", "cpu", "gpu", "usb", "pdf", "faq", "ceo", "cto",
    "cfo", "vip", "asap", "diy", "suv", "gdp", "dna", "rna", "hiv", "nba", "nfl", "ussr",
    // Russian initialisms
    "сша", "ссср", "рф", "мгу", "мид", "ндс", "ржд", "гибдд", "оао", "ооо", "зао",
];

fn is_acronym(core: &str) -> bool {
    !core.is_empty() && ACRONYMS.contains(&core.to_lowercase().as_str())
}

/// Uppercases every word whose core is a known acronym, preserving the
/// leading/trailing punctuation exactly.
pub fn normalize_abbreviations(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let (core, prefix, suffix) = split_punct(word);
            if is_acronym(core) {
                format!("{prefix}{}{suffix}", core.to_uppercase())
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_acronyms() {
        assert_eq!(normalize_abbreviations("nasa"), "NASA");
        assert_eq!(normalize_abbreviations("the fbi and the cia"), "the FBI and the CIA");
        assert_eq!(normalize_abbreviations("json over http"), "JSON over HTTP");
    }

    #[test]
    fn russian_acronyms() {
        assert_eq!(normalize_abbreviations("сша и ссср"), "США и СССР");
        assert_eq!(normalize_abbreviations("ндс"), "НДС");
    }

    #[test]
    fn mixed_case_input_is_normalized() {
        assert_eq!(normalize_abbreviations("Nasa"), "NASA");
        assert_eq!(normalize_abbreviations("Http"), "HTTP");
    }

    #[test]
    fn preserves_surrounding_punctuation() {
        assert_eq!(normalize_abbreviations("(nasa)"), "(NASA)");
        assert_eq!(normalize_abbreviations("nasa,"), "NASA,");
        assert_eq!(normalize_abbreviations("работает в мид."), "работает в МИД.");
    }

    #[test]
    fn leaves_ordinary_words_untouched() {
        assert_eq!(normalize_abbreviations("this is a test"), "this is a test");
        assert_eq!(normalize_abbreviations("hello world"), "hello world");
        assert_eq!(normalize_abbreviations(""), "");
    }
}
