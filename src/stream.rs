#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStreamEvent {
    Status(String),
    ToolStarted { command: String },
    ToolOutput(String),
    ToolCompleted { exit_code: Option<i64> },
    AssistantText(String),
    Usage(TokenUsage),
}

impl AgentStreamEvent {
    #[must_use]
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::AssistantText(text.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: i64,
    pub reasoning_output_tokens: Option<i64>,
}
