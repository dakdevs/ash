use crate::shell::LexedWord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellProgram {
    Simple(SimpleCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    pub assignments: Vec<(String, String)>,
    pub words: Vec<LexedWord>,
}

impl SimpleCommand {
    #[must_use]
    pub fn new(assignments: Vec<(String, String)>, words: Vec<LexedWord>) -> Self {
        Self { assignments, words }
    }
}
