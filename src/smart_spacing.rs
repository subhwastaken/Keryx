#![allow(dead_code)]

//! Context-Aware Smart Spacing Engine
//! Prevents glued words ("helloworld") while keeping punctuation tightly bound ("hello, world").

/// Adjusts leading and trailing spacing based on surrounding text context
pub fn apply_smart_spacing(new_text: &str, preceding_char: Option<char>) -> String {
    let trimmed = new_text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let first_char = trimmed.chars().next().unwrap();

    // 1. If new text starts with punctuation, attach tightly without space
    let is_punct = matches!(first_char, '.' | ',' | '!' | '?' | ':' | ';' | ')' | ']' | '}' | '%' | '\'');
    if is_punct {
        return trimmed.to_string();
    }

    // 2. Check preceding context (if known)
    match preceding_char {
        Some(prev) => {
            // If preceding is alphanumeric or word-ending, insert space
            if prev.is_alphanumeric() || matches!(prev, '.' | ',' | '!' | '?' | ':' | ';' | ')' | ']' | '}') {
                format!(" {}", trimmed)
            } else if prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | '"' | '\'') {
                // Preceding is space, newline, or open delimiter -> no extra space
                trimmed.to_string()
            } else {
                format!(" {}", trimmed)
            }
        }
        None => {
            // Default safe mode: if string already starts with whitespace, preserve single space
            if new_text.starts_with(char::is_whitespace) {
                format!(" {}", trimmed)
            } else {
                trimmed.to_string()
            }
        }
    }
}

/// Normalizes paste text to ensure proper comma spacing without regex
pub fn normalize_paste_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let chars: Vec<char> = text.trim().chars().collect();

    for i in 0..chars.len() {
        out.push(chars[i]);
        if chars[i] == ',' && i + 1 < chars.len() && !chars[i + 1].is_whitespace() && chars[i + 1].is_alphanumeric() {
            out.push(' ');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_punctuation_tight_attachment() {
        let result = apply_smart_spacing(" , world", Some('o'));
        assert_eq!(result, ", world");

        let result2 = apply_smart_spacing("? That's great", Some('y'));
        assert_eq!(result2, "? That's great");
    }

    #[test]
    fn test_word_separation() {
        let result = apply_smart_spacing("world", Some('o'));
        assert_eq!(result, " world");

        let result_space = apply_smart_spacing("world", Some(' '));
        assert_eq!(result_space, "world");

        let result_newline = apply_smart_spacing("world", Some('\n'));
        assert_eq!(result_newline, "world");
    }

    #[test]
    fn test_open_brackets() {
        let result = apply_smart_spacing("world", Some('('));
        assert_eq!(result, "world");
    }
}
