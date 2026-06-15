use crate::{
    error::{AshError, Result},
    shell::{Lexer, ShellProgram, SimpleCommand},
};

#[derive(Debug, Default)]
pub struct Parser;

impl Parser {
    pub fn parse(input: &str) -> Result<ShellProgram> {
        let words = Lexer::lex(input)?;
        if words.is_empty() {
            return Err(AshError::EmptyCommand);
        }

        let mut assignments = Vec::new();
        let mut command_words = Vec::new();

        for word in words {
            if command_words.is_empty()
                && is_assignment(&word.text)
                && !word.text.starts_with('=')
                && !word.text.contains("$=")
            {
                if let Some((key, value)) = word.text.split_once('=') {
                    assignments.push((key.to_owned(), value.to_owned()));
                }
            } else {
                command_words.push(word);
            }
        }

        Ok(ShellProgram::Simple(SimpleCommand::new(
            assignments,
            command_words,
        )))
    }
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };

    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use crate::shell::{Parser, ShellProgram};

    #[test]
    fn parses_assignments_before_command_words() {
        let program = Parser::parse("NAME=ash echo hi").expect("parse");

        let ShellProgram::Simple(command) = program;
        assert_eq!(
            command.assignments,
            vec![("NAME".to_owned(), "ash".to_owned())]
        );
        assert_eq!(
            command
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>(),
            vec!["echo", "hi"]
        );
    }
}
