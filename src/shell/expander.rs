use std::collections::HashMap;

use crate::shell::LexedWord;

#[derive(Debug, Clone)]
pub struct Expander {
    environment: HashMap<String, String>,
}

impl Expander {
    #[must_use]
    pub fn new(environment: HashMap<String, String>) -> Self {
        Self { environment }
    }

    #[must_use]
    pub fn expand_words(&self, words: Vec<LexedWord>) -> Vec<String> {
        words
            .into_iter()
            .map(|word| {
                if word.expand_env {
                    self.expand_env(&word.text)
                } else {
                    word.text
                }
            })
            .collect()
    }

    fn expand_env(&self, input: &str) -> String {
        let mut output = String::new();
        let mut characters = input.chars().peekable();

        while let Some(character) = characters.next() {
            if character != '$' {
                output.push(character);
                continue;
            }

            if characters.peek() == Some(&'{') {
                characters.next();
                let mut name = String::new();
                for next in characters.by_ref() {
                    if next == '}' {
                        break;
                    }
                    name.push(next);
                }
                output.push_str(self.environment.get(&name).map_or("", String::as_str));
                continue;
            }

            let mut name = String::new();
            while let Some(next) = characters.peek().copied() {
                if next == '_' || next.is_ascii_alphanumeric() {
                    name.push(next);
                    characters.next();
                } else {
                    break;
                }
            }

            if name.is_empty() {
                output.push('$');
            } else {
                output.push_str(self.environment.get(&name).map_or("", String::as_str));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::shell::LexedWord;

    use super::Expander;

    #[test]
    fn expands_environment_variables_when_allowed() {
        let expander = Expander::new(HashMap::from([("NAME".to_owned(), "ASH".to_owned())]));
        let words = expander.expand_words(vec![
            LexedWord {
                text: "hello-$NAME".to_owned(),
                expand_env: true,
            },
            LexedWord {
                text: "$NAME".to_owned(),
                expand_env: false,
            },
        ]);

        assert_eq!(words, vec!["hello-ASH", "$NAME"]);
    }
}
