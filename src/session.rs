use std::path::PathBuf;

use crate::{
    agent::Agent,
    config::{AshConfig, ModePersistence},
    context::{ContextEvent, ContextStore},
    error::{AshError, Result},
    shell::{ExecutionResult, ShellExecutor},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    Agent,
    Command,
}

impl PromptMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "agent" | ">" => Ok(Self::Agent),
            "command" | "$" => Ok(Self::Command),
            other => Err(AshError::UnknownPromptMode(other.to_owned())),
        }
    }

    #[must_use]
    pub const fn prompt(self) -> &'static str {
        match self {
            Self::Agent => ">",
            Self::Command => "$",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionResponse {
    Agent(String),
    Command(ExecutionResult),
    ModeChanged(PromptMode),
    Empty,
}

pub struct AshSession<S, A>
where
    S: ContextStore,
    A: Agent,
{
    config: AshConfig,
    context: S,
    agent: A,
    shell: ShellExecutor,
    mode: PromptMode,
}

impl<S, A> AshSession<S, A>
where
    S: ContextStore,
    A: Agent,
{
    pub fn new(config: AshConfig, context: S, agent: A, cwd: PathBuf) -> Self {
        let mode = config.default_mode;
        Self {
            config,
            context,
            agent,
            shell: ShellExecutor::new(cwd),
            mode,
        }
    }

    pub const fn mode(&self) -> PromptMode {
        self.mode
    }

    pub fn prompt(&self) -> String {
        format!("{} ", self.mode.prompt())
    }

    pub fn status_line(&self) -> String {
        format!(
            "[ash mode={} provider={} cwd={}]",
            self.mode.prompt(),
            self.config.default_provider,
            self.shell.cwd().display()
        )
    }

    pub fn toggle_mode(&mut self) -> Result<SessionResponse> {
        self.mode = match self.mode {
            PromptMode::Agent => PromptMode::Command,
            PromptMode::Command => PromptMode::Agent,
        };
        self.context.record(ContextEvent::mode_changed(self.mode))?;
        Ok(SessionResponse::ModeChanged(self.mode))
    }

    pub fn handle_line(&mut self, line: &str) -> Result<SessionResponse> {
        let input = line.trim_end_matches(['\r', '\n']);
        if input.is_empty() {
            return Ok(SessionResponse::Empty);
        }
        if input == "\t" {
            return self.toggle_mode();
        }

        match self.mode {
            PromptMode::Agent => self.handle_agent_prompt(input),
            PromptMode::Command => self.handle_command(input),
        }
    }

    fn handle_agent_prompt(&mut self, input: &str) -> Result<SessionResponse> {
        self.context.record(ContextEvent::agent_prompt(input))?;
        let response = self.agent.respond(input)?;
        self.context
            .record(ContextEvent::agent_response(&response))?;
        Ok(SessionResponse::Agent(response))
    }

    fn handle_command(&mut self, input: &str) -> Result<SessionResponse> {
        self.context.record(ContextEvent::command_input(input))?;
        let result = self.shell.execute_line(input)?;
        self.context.record(ContextEvent::command_result(&result))?;

        if self.config.command_mode == ModePersistence::OneShot {
            self.mode = PromptMode::Agent;
        }

        Ok(SessionResponse::Command(result))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        agent::EchoAgent,
        config::{AshConfig, ModePersistence},
        context::InMemoryContextStore,
    };

    use super::{AshSession, PromptMode, SessionResponse};

    #[test]
    fn tab_on_empty_line_toggles_modes() {
        let mut session = AshSession::new(
            AshConfig::default(),
            InMemoryContextStore::default(),
            EchoAgent,
            std::env::current_dir().expect("cwd"),
        );

        assert_eq!(session.mode(), PromptMode::Agent);
        assert_eq!(
            session.handle_line("\t").expect("toggle"),
            SessionResponse::ModeChanged(PromptMode::Command)
        );
        assert_eq!(session.mode(), PromptMode::Command);
    }

    #[test]
    fn one_shot_command_mode_returns_to_agent_mode() {
        let config = AshConfig {
            default_mode: PromptMode::Command,
            command_mode: ModePersistence::OneShot,
            ..AshConfig::default()
        };
        let mut session = AshSession::new(
            config,
            InMemoryContextStore::default(),
            EchoAgent,
            std::env::current_dir().expect("cwd"),
        );

        let response = session.handle_line("pwd").expect("command");

        assert!(matches!(response, SessionResponse::Command(_)));
        assert_eq!(session.mode(), PromptMode::Agent);
    }
}
