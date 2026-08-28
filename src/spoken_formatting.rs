#![allow(dead_code)]

//! Smart Spoken Formatting & Punctuation Engine
//! Converts spoken commands like "new line", "new paragraph", "bullet point",
//! "colon", "open quote" into clean markdown and punctuation layout without external dependencies.

/// Replaces spoken formatting and punctuation commands with clean symbols
pub fn format_spoken_commands(input: &str) -> String {
    let words: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
    if words.is_empty() {
        return String::new();
    }

    let mut result_tokens: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        let w1 = words[i].to_lowercase();
        let w2 = if i + 1 < words.len() {
            words[i + 1].to_lowercase()
        } else {
            String::new()
        };
        let two_words = format!("{} {}", w1, w2);

        // Check two-word phrases
        match two_words.as_str() {
            "new line" | "next line" => {
                result_tokens.push("\n".to_string());
                i += 2;
                continue;
            }
            "new paragraph" | "next paragraph" => {
                result_tokens.push("\n\n".to_string());
                i += 2;
                continue;
            }
            "bullet point" | "bullet item" | "new bullet" => {
                result_tokens.push("\n• ".to_string());
                i += 2;
                continue;
            }
            "full stop" => {
                result_tokens.push(".".to_string());
                i += 2;
                continue;
            }
            "question mark" => {
                result_tokens.push("?".to_string());
                i += 2;
                continue;
            }
            "exclamation mark" | "exclamation point" => {
                result_tokens.push("!".to_string());
                i += 2;
                continue;
            }
            "open quote" | "open quotation" => {
                result_tokens.push("\"".to_string());
                i += 2;
                continue;
            }
            "close quote" | "close quotation" => {
                result_tokens.push("\"_close".to_string());
                i += 2;
                continue;
            }
            "open paren" | "open parenthesis" => {
                result_tokens.push("(".to_string());
                i += 2;
                continue;
            }
            "close paren" | "close parenthesis" => {
                result_tokens.push(")_close".to_string());
                i += 2;
                continue;
            }
            "open bracket" => {
                result_tokens.push("[".to_string());
                i += 2;
                continue;
            }
            "close bracket" => {
                result_tokens.push("]_close".to_string());
                i += 2;
                continue;
            }
            "open brace" | "open curly" => {
                result_tokens.push("{".to_string());
                i += 2;
                continue;
            }
            "close brace" | "close curly" => {
                result_tokens.push("}_close".to_string());
                i += 2;
                continue;
            }
            "dot dot" => {
                result_tokens.push("...".to_string());
                i += 2;
                continue;
            }
            "smiley face" => {
                result_tokens.push(":)".to_string());
                i += 2;
                continue;
            }
            "sad face" => {
                result_tokens.push(":(".to_string());
                i += 2;
                continue;
            }
            "heart emoji" => {
                result_tokens.push("❤️".to_string());
                i += 2;
                continue;
            }
            _ => {}
        }

        // Check single-word phrases
        match w1.as_str() {
            "newline" => {
                result_tokens.push("\n".to_string());
                i += 1;
            }
            "period" => {
                result_tokens.push(".".to_string());
                i += 1;
            }
            "comma" => {
                result_tokens.push(",".to_string());
                i += 1;
            }
            "colon" => {
                result_tokens.push(":".to_string());
                i += 1;
            }
            "semicolon" => {
                result_tokens.push(";".to_string());
                i += 1;
            }
            "hyphen" | "dash" => {
                result_tokens.push("-".to_string());
                i += 1;
            }
            _ => {
                result_tokens.push(words[i].clone());
                i += 1;
            }
        }
    }

    assemble_formatted_tokens(&result_tokens)
}

fn assemble_formatted_tokens(tokens: &[String]) -> String {
    let mut out = String::new();
    let mut cap_next = true;

    for token in tokens {
        if token == "\n" || token == "\n\n" {
            out.push_str(token);
            cap_next = true;
            continue;
        }

        if token == "\n• " {
            out.push_str("\n• ");
            cap_next = true;
            continue;
        }

        let (actual_token, is_close_marker) = if let Some(stripped) = token.strip_suffix("_close") {
            (stripped, true)
        } else {
            (token.as_str(), false)
        };

        let is_punct = matches!(actual_token, "." | "," | "!" | "?" | ":" | ";" | ")" | "]" | "}");

        if !is_punct && !is_close_marker && !out.is_empty() && !out.ends_with('\n') && !out.ends_with('•') && !out.ends_with(' ') && !out.ends_with('(') && !out.ends_with('[') && !out.ends_with('{') && !out.ends_with('"') {
            out.push(' ');
        }

        if cap_next {
            let mut chars = actual_token.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
            cap_next = false;
        } else {
            out.push_str(actual_token);
        }

        if matches!(actual_token, "." | "!" | "?") {
            cap_next = true;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_newline_and_paragraph() {
        let input = "hello world new line this is line two new paragraph here is paragraph two";
        let output = format_spoken_commands(input);
        assert!(output.contains("\nThis is line two"));
        assert!(output.contains("\n\nHere is paragraph two"));
    }

    #[test]
    fn test_punctuation_and_quotes() {
        let input = "he said open quote welcome home close quote exclamation mark";
        let output = format_spoken_commands(input);
        assert_eq!(output, "He said \"welcome home\"!");
    }

    #[test]
    fn test_bullets_and_colons() {
        let input = "requirements colon bullet point fast speed bullet point reliable";
        let output = format_spoken_commands(input);
        assert!(output.contains("Requirements:"));
        assert!(output.contains("\n• Fast speed"));
        assert!(output.contains("\n• Reliable"));
    }
}
