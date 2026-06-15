use crate::error::{AshError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexedWord {
    pub text: String,
    pub expand_env: bool,
}

#[derive(Debug, Default)]
pub struct Lexer;

impl Lexer {
    pub fn lex(input: &str) -> Result<Vec<LexedWord>> {
        let mut words = Vec::new();
        let mut text = String::new();
        let mut quote = Quote::None;
        let mut expand_env = false;
        let mut escaped = false;

        for character in input.chars() {
            if escaped {
                text.push(character);
                escaped = false;
                continue;
            }

            match (quote, character) {
                (_, '\\') if quote != Quote::Single => escaped = true,
                (Quote::None, '\'') => quote = Quote::Single,
                (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
                (Quote::None, '"') => {
                    quote = Quote::Double;
                    expand_env = true;
                }
                (Quote::None, character) if character.is_whitespace() => {
                    push_word(&mut words, &mut text, expand_env);
                    expand_env = false;
                }
                (Quote::Single, character) => text.push(character),
                (_, character) => {
                    if character == '$' {
                        expand_env = true;
                    }
                    text.push(character);
                }
            }
        }

        if escaped {
            text.push('\\');
        }

        if quote != Quote::None {
            return Err(AshError::UnterminatedQuote);
        }

        push_word(&mut words, &mut text, expand_env);
        Ok(words)
    }
}

fn push_word(words: &mut Vec<LexedWord>, text: &mut String, expand_env: bool) {
    if !text.is_empty() {
        words.push(LexedWord {
            text: std::mem::take(text),
            expand_env,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

#[cfg(test)]
mod tests {
    use super::Lexer;

    #[test]
    fn lexes_quotes_and_whitespace() {
        let words = Lexer::lex(r#"echo "hello world" 'no $expand'"#).expect("lex");

        assert_eq!(words[0].text, "echo");
        assert_eq!(words[1].text, "hello world");
        assert!(words[1].expand_env);
        assert_eq!(words[2].text, "no $expand");
        assert!(!words[2].expand_env);
    }
}
