//! Command-line tokenization.
//!
//! Ported from the Java `Tokenizer` behavior needed by commands: whitespace
//! separates arguments, single/double quotes group a token, and a backslash
//! escapes the next character. A leading `/` is accepted for convenience.

use thiserror::Error;

/// A malformed command line.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenizeError {
    /// A quote was opened but never closed.
    #[error("unterminated quote")]
    UnterminatedQuote,
    /// A backslash was the last character of the input.
    #[error("trailing escape")]
    TrailingEscape,
}

/// Split a command line into tokens.
pub fn tokenize(input: &str) -> Result<Vec<String>, TokenizeError> {
    let input = input.trim_start_matches('/').trim();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(character) = chars.next() {
        match quote {
            Some(delimiter) => {
                if character == delimiter {
                    quote = None;
                    in_token = true;
                } else if character == '\\' {
                    let Some(escaped) = chars.next() else {
                        return Err(TokenizeError::TrailingEscape);
                    };
                    current.push(escaped);
                    in_token = true;
                } else {
                    current.push(character);
                }
            }
            None => match character {
                '"' | '\'' => {
                    quote = Some(character);
                    in_token = true;
                }
                '\\' => {
                    let Some(escaped) = chars.next() else {
                        return Err(TokenizeError::TrailingEscape);
                    };
                    current.push(escaped);
                    in_token = true;
                }
                character if character.is_whitespace() => {
                    if in_token {
                        tokens.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                }
                character => {
                    current.push(character);
                    in_token = true;
                }
            },
        }
    }

    if quote.is_some() {
        return Err(TokenizeError::UnterminatedQuote);
    }
    if in_token {
        tokens.push(current);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{tokenize, TokenizeError};

    #[test]
    fn splits_plain_tokens_and_strips_a_leading_slash() {
        assert_eq!(
            tokenize("/tp 1.5 64 -2").expect("tokens"),
            ["tp", "1.5", "64", "-2"]
        );
    }

    #[test]
    fn supports_quotes_and_escapes() {
        assert_eq!(
            tokenize(r#"/give "Rock Stone" 2"#).expect("tokens"),
            ["give", "Rock Stone", "2"]
        );
        assert_eq!(
            tokenize(r#"/say "a \"quoted\" word""#).expect("tokens"),
            ["say", "a \"quoted\" word"]
        );
        assert_eq!(
            tokenize(r#"/say 'single quoted'"#).expect("tokens"),
            ["say", "single quoted"]
        );
        assert_eq!(
            tokenize(r#"/say escaped\ space"#).expect("tokens"),
            ["say", "escaped space"]
        );
    }

    #[test]
    fn keeps_empty_quoted_arguments() {
        assert_eq!(tokenize(r#"/say """#).expect("tokens"), ["say", ""]);
    }

    #[test]
    fn rejects_unterminated_quotes_and_escapes() {
        assert_eq!(
            tokenize(r#"/say "unterminated"#),
            Err(TokenizeError::UnterminatedQuote)
        );
        assert_eq!(tokenize("/say \\"), Err(TokenizeError::TrailingEscape));
    }
}
