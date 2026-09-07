//! Spoken arithmetic rewritten as symbolic notation (English + Russian).
//!
//! Three related shapes, each anchored on a number resolved through the
//! shared `numbers::parse_number_at` (so an operand may be a digit or a
//! spelled-out cardinal, same as every other extended pass):
//!   - a binary chain: "<n> <op-word> <n> (<op-word> <n>)*" -> symbols
//!     spliced in for the operator words ("two plus three" -> "2 + 3",
//!     "two plus three equals five" -> "2 + 3 = 5");
//!   - a postfix power: "<n> squared"/"<n> в квадрате" -> "<n>²",
//!     "<n> cubed"/"<n> в кубе" -> "<n>³";
//!   - a prefix root: "square root of <n>"/"корень из <n>" -> "√<n>".
//!
//! Deliberately conservative: every shape requires a complete, unambiguous
//! match (a number on *every* required side) before anything is rewritten,
//! so an operator word used as ordinary prose - "the profit was minus five
//! dollars" (no number before "minus"), "three squares on the table"
//! ("squares" isn't "squared") - is left exactly as spoken. This mirrors
//! how the currency/percent passes only fire when their trailing unit word
//! actually follows a number.

use super::numbers::parse_number_at;
use super::split_punct;

/// A recognized binary operator, alongside its output symbol.
#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
}

fn op_symbol(op: Op) -> &'static str {
    match op {
        Op::Add => "+",
        Op::Sub => "\u{2212}", // − (minus sign, distinct from an ASCII hyphen)
        Op::Mul => "×",
        Op::Div => "÷",
        Op::Eq => "=",
    }
}

/// Tries to match an operator phrase (one or two words) at `cores[i]`.
/// Two-word phrases are checked first so "multiplied"/"divided" alone
/// never partially matches.
fn match_operator(cores: &[&str], i: usize) -> Option<(Op, usize)> {
    let w0 = cores.get(i)?.to_lowercase();
    if let Some(w1) = cores.get(i + 1).map(|s| s.to_lowercase()) {
        match (w0.as_str(), w1.as_str()) {
            ("multiplied", "by") => return Some((Op::Mul, 2)),
            ("divided", "by") => return Some((Op::Div, 2)),
            ("умножить", "на") => return Some((Op::Mul, 2)),
            ("разделить", "на") => return Some((Op::Div, 2)),
            _ => {}
        }
    }
    Some((
        match w0.as_str() {
            "plus" | "плюс" => Op::Add,
            "minus" | "минус" => Op::Sub,
            "times" => Op::Mul,
            "equals" | "равно" => Op::Eq,
            _ => return None,
        },
        1,
    ))
}

/// Tries to match a postfix power word/phrase ("squared"/"в квадрате",
/// "cubed"/"в кубе") at `cores[i]`, returning the superscript character and
/// how many words it consumed.
fn match_postfix_power(cores: &[&str], i: usize) -> Option<(char, usize)> {
    let w0 = cores.get(i)?.to_lowercase();
    match w0.as_str() {
        "squared" => return Some(('²', 1)),
        "cubed" => return Some(('³', 1)),
        _ => {}
    }
    if w0 == "в" {
        match cores.get(i + 1).map(|s| s.to_lowercase()).as_deref() {
            Some("квадрате") => return Some(('²', 2)),
            Some("кубе") => return Some(('³', 2)),
            _ => {}
        }
    }
    None
}

/// Tries to match the "square root of" / "корень из" prefix at `cores[i]`,
/// returning how many words the prefix phrase itself consumed (not
/// counting the operand that follows).
fn match_sqrt_prefix(cores: &[&str], i: usize) -> Option<usize> {
    let w0 = cores.get(i)?.to_lowercase();
    if w0 == "square"
        && cores.get(i + 1).map(|s| s.to_lowercase()).as_deref() == Some("root")
        && cores.get(i + 2).map(|s| s.to_lowercase()).as_deref() == Some("of")
    {
        return Some(3);
    }
    if w0 == "корень" && cores.get(i + 1).map(|s| s.to_lowercase()).as_deref() == Some("из")
    {
        return Some(2);
    }
    None
}

/// Russian genitive-case single/teen number words, as required after
/// "из" ("корень из девяти" - "of nine"). `numbers::parse_number_at` only
/// knows the nominative forms ("девять"), so the root operand needs this
/// small dedicated table; English needs no equivalent since "nine" doesn't
/// change form after "of".
fn genitive_number(core: &str) -> Option<u64> {
    Some(match core.to_lowercase().as_str() {
        "нуля" | "ноля" => 0,
        "одного" | "одной" => 1,
        "двух" => 2,
        "трёх" | "трех" => 3,
        "четырёх" | "четырех" => 4,
        "пяти" => 5,
        "шести" => 6,
        "семи" => 7,
        "восьми" => 8,
        "девяти" => 9,
        "десяти" => 10,
        "одиннадцати" => 11,
        "двенадцати" => 12,
        "тринадцати" => 13,
        "четырнадцати" => 14,
        "пятнадцати" => 15,
        "шестнадцати" => 16,
        "семнадцати" => 17,
        "восемнадцати" => 18,
        "девятнадцати" => 19,
        "двадцати" => 20,
        _ => return None,
    })
}

/// Resolves the operand right after a "square root of"/"корень из" prefix:
/// a digit, a nominative cardinal (covers English, and a Russian operand
/// that already arrived as a digit), or - Russian only - a genitive number
/// word.
fn root_operand(cores: &[&str], i: usize) -> Option<(u64, usize)> {
    if let Some(hit) = parse_number_at(cores, i) {
        return Some(hit);
    }
    genitive_number(cores.get(i)?).map(|v| (v, 1))
}

/// Replaces every recognized formula shape in `text` with its symbolic
/// form, leaving everything else untouched.
pub fn normalize_formulas(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some(prefix_len) = match_sqrt_prefix(&cores, i) {
            if let Some((value, num_len)) = root_operand(&cores, i + prefix_len) {
                let end = i + prefix_len + num_len;
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[end - 1]);
                out.push(format!("{prefix}\u{221a}{value}{suffix}"));
                i = end;
                continue;
            }
        }

        if let Some((first_value, num_len)) = parse_number_at(&cores, i) {
            let end = i + num_len;

            if let Some((power, plen)) = match_postfix_power(&cores, end) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[end + plen - 1]);
                out.push(format!("{prefix}{first_value}{power}{suffix}"));
                i = end + plen;
                continue;
            }

            let mut chain = first_value.to_string();
            let mut cursor = end;
            let mut matched_any = false;
            while let Some((op, oplen)) = match_operator(&cores, cursor) {
                let Some((rhs, rhs_len)) = parse_number_at(&cores, cursor + oplen) else {
                    break;
                };
                chain.push(' ');
                chain.push_str(op_symbol(op));
                chain.push(' ');
                chain.push_str(&rhs.to_string());
                cursor += oplen + rhs_len;
                matched_any = true;
            }
            if matched_any {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[cursor - 1]);
                out.push(format!("{prefix}{chain}{suffix}"));
                i = cursor;
                continue;
            }
        }

        out.push(words[i].to_string());
        i += 1;
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addition() {
        assert_eq!(normalize_formulas("two plus three"), "2 + 3");
        assert_eq!(normalize_formulas("два плюс три"), "2 + 3");
    }

    #[test]
    fn subtraction() {
        assert_eq!(normalize_formulas("five minus two"), "5 \u{2212} 2");
        assert_eq!(normalize_formulas("пять минус два"), "5 \u{2212} 2");
    }

    #[test]
    fn multiplication() {
        assert_eq!(normalize_formulas("four times two"), "4 × 2");
        assert_eq!(normalize_formulas("four multiplied by two"), "4 × 2");
        assert_eq!(normalize_formulas("четыре умножить на два"), "4 × 2");
    }

    #[test]
    fn division() {
        assert_eq!(normalize_formulas("eight divided by two"), "8 ÷ 2");
        assert_eq!(normalize_formulas("восемь разделить на два"), "8 ÷ 2");
    }

    #[test]
    fn equals_chains_a_full_expression() {
        assert_eq!(
            normalize_formulas("two plus three equals five"),
            "2 + 3 = 5"
        );
        assert_eq!(normalize_formulas("два плюс три равно пять"), "2 + 3 = 5");
    }

    #[test]
    fn mixed_digit_and_word_operands() {
        assert_eq!(normalize_formulas("5 plus three"), "5 + 3");
        assert_eq!(normalize_formulas("two plus 3"), "2 + 3");
    }

    #[test]
    fn squares_and_cubes() {
        assert_eq!(normalize_formulas("five squared"), "5²");
        assert_eq!(normalize_formulas("пять в квадрате"), "5²");
        assert_eq!(normalize_formulas("two cubed"), "2³");
        assert_eq!(normalize_formulas("два в кубе"), "2³");
    }

    #[test]
    fn square_roots() {
        assert_eq!(normalize_formulas("square root of nine"), "\u{221a}9");
        assert_eq!(normalize_formulas("корень из девяти"), "\u{221a}9");
        assert_eq!(normalize_formulas("square root of 16"), "\u{221a}16");
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_formulas("I calculated two plus three today."),
            "I calculated 2 + 3 today."
        );
        assert_eq!(normalize_formulas("(two plus three)"), "(2 + 3)");
    }

    #[test]
    fn leaves_incomplete_expressions_alone() {
        // "times" with nothing valid after it is not a formula - the
        // leading number is still passed through untouched (unconverted),
        // since this pass on its own doesn't digitize bare numbers.
        assert_eq!(normalize_formulas("three times a day"), "three times a day");
    }

    #[test]
    fn leaves_ordinary_prose_alone() {
        // No number precedes "minus" here, so it's left as an ordinary word.
        assert_eq!(
            normalize_formulas("the profit was minus five dollars"),
            "the profit was minus five dollars"
        );
        // "squares" (plural noun) is not "squared" (the operator word).
        assert_eq!(
            normalize_formulas("three squares on the table"),
            "three squares on the table"
        );
        // "square" alone, with no "root of" after it, is an ordinary word.
        assert_eq!(normalize_formulas("a square room"), "a square room");
        assert_eq!(normalize_formulas("hello world"), "hello world");
        assert_eq!(normalize_formulas(""), "");
    }
}
