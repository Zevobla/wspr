//! Service token and tag stripping for LLM outputs.
//! Removes model-specific markers like <think>...</think>, <|...|> tokens,
//! and other service tags that should not appear in user-facing output.

/// Strips service tags and special tokens from LLM output.
/// Removes:
/// - Thinking tags: <think>...</think>, <reasoning>...</reasoning>, etc., including their content
/// - Service tokens: <|...|>, <...>
/// - Dictionary echoes and other noise
pub fn strip_special_tokens(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Try to consume a complete tag/token
            let mut tag = String::from("<");
            let mut found_close = false;

            while let Some(&next_ch) = chars.peek() {
                tag.push(chars.next().unwrap());
                if next_ch == '>' {
                    found_close = true;
                    break;
                }
                // Limit tag length to avoid consuming too much on malformed input
                if tag.len() > 100 {
                    break;
                }
            }

            // If we found a closing >, check if it's a tag we should skip
            if found_close {
                let tag_inner = &tag[1..tag.len() - 1]; // Extract content between < and >

                // Check for various types of tags to skip
                let should_skip_and_content = is_opening_service_tag(tag_inner);
                let should_skip_tag_only = tag_inner.is_empty() // Empty <>
                    || tag_inner.starts_with('|') // <|...|> tokens
                    || tag_inner.starts_with('/') // Closing tags like </think>
                    || is_service_tag(tag_inner);

                if should_skip_and_content {
                    // This is an opening service tag like <think>, <reasoning>, etc.
                    // Skip everything until the corresponding closing tag
                    let close_tag = format!("</{}>", tag_inner);
                    let mut depth = 1;
                    while depth > 0 {
                        match chars.next() {
                            None => break,
                            Some(c) => {
                                if c == '<' {
                                    let mut potential_tag = String::from("<");
                                    while let Some(&next_ch) = chars.peek() {
                                        potential_tag.push(chars.next().unwrap());
                                        if next_ch == '>' {
                                            break;
                                        }
                                        if potential_tag.len() > 100 {
                                            break;
                                        }
                                    }
                                    if potential_tag == close_tag {
                                        depth -= 1;
                                    } else if potential_tag == tag {
                                        depth += 1;
                                    }
                                }
                            }
                        }
                    }
                } else if !should_skip_tag_only {
                    // Not a service tag, keep it
                    result.push_str(&tag);
                }
                // Otherwise skip the tag entirely
            } else {
                // No closing >, keep the < and what we consumed
                result.push_str(&tag);
            }
        } else {
            result.push(ch);
        }
    }

    // Normalize whitespace: collapse multiple spaces into single space
    result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")

}

/// Checks if the tag content (without < and >) is an opening service tag that has content to skip.
fn is_opening_service_tag(tag_inner: &str) -> bool {
    let lower = tag_inner.to_lowercase();

    // Opening tags whose content should be skipped entirely
    matches!(
        lower.as_str(),
        "think" | "reasoning" | "analysis" | "reflection" | "summary" | "output"
    )
}

/// Checks if the tag content (without < and >) is a known service tag.
fn is_service_tag(tag_inner: &str) -> bool {
    let lower = tag_inner.to_lowercase();

    // Common model service tags
    matches!(
        lower.as_str(),
        "think" | "/think"
            | "reasoning" | "/reasoning"
            | "analysis" | "/analysis"
            | "reflection" | "/reflection"
            | "summary" | "/summary"
            | "end" | "/end"
            | "eog" | "/eog" // End of generation
            | "eos" | "/eos" // End of sequence
            | "bos" | "/bos" // Beginning of sequence
            | "pad" | "/pad" // Padding
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_think_tags_and_service_tokens() {
        let input = "<think>reasoning</think>Hello <|end|>";
        let result = strip_special_tokens(input);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn strips_multiple_service_tags() {
        let input = "<think>internal</think> text <reasoning>more</reasoning> output";
        let result = strip_special_tokens(input);
        // Both think and reasoning tags and their content are removed
        assert_eq!(result, "text output");
    }

    #[test]
    fn strips_pipe_tokens() {
        let input = "hello <|begin|> world <|end|>";
        let result = strip_special_tokens(input);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn preserves_regular_angle_brackets() {
        // User text with angle brackets should be preserved
        let input = "use 2<x<5 in the formula";
        let result = strip_special_tokens(input);
        assert_eq!(result, "use 2<x<5 in the formula");
    }

    #[test]
    fn handles_empty_tags() {
        let input = "text <> more";
        let result = strip_special_tokens(input);
        assert_eq!(result, "text more");
    }

    #[test]
    fn handles_malformed_tags() {
        // Unclosed tag - keep it since we can't identify it
        let input = "text <unclosed more";
        let result = strip_special_tokens(input);
        assert_eq!(result, "text <unclosed more");
    }

    #[test]
    fn strips_closing_tags() {
        let input = "start</think>middle</reasoning>end";
        let result = strip_special_tokens(input);
        assert_eq!(result, "startmiddleend");
    }

    #[test]
    fn trims_whitespace() {
        let input = "  text  <think>hidden</think>  more  ";
        let result = strip_special_tokens(input);
        // Leading/trailing whitespace trimmed
        assert_eq!(result, "text more");
    }
}
