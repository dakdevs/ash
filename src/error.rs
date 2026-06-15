use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, AshError>;

#[derive(Debug, thiserror::Error)]
pub enum AshError {
    #[error("failed to parse .ashrc line {line}: {message}")]
    AshrcParse { line: usize, message: String },

    #[error("unterminated quote in shell input")]
    UnterminatedQuote,

    #[error("empty shell command")]
    EmptyCommand,

    #[error("unknown prompt mode: {0}")]
    UnknownPromptMode(String),

    #[error("unknown permission action: {0}")]
    UnknownPermissionAction(String),

    #[error("provider `{0}` is not configured")]
    ProviderNotConfigured(String),

    #[error("codex executable was not found")]
    CodexNotFound,

    #[error("process `{program}` failed to start: {source}")]
    ProcessSpawn {
        program: String,
        source: std::io::Error,
    },

    #[error("process `{program}` failed while waiting: {source}")]
    ProcessWait {
        program: String,
        source: std::io::Error,
    },

    #[error("failed to change directory to `{path}`: {source}")]
    ChangeDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("database connection error: {0}")]
    DatabaseConnection(#[from] diesel::ConnectionError),

    #[error("database migration error: {0}")]
    DatabaseMigration(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
