use std::path::PathBuf;

use crate::{
    agent::Agent,
    config::{AshConfig, ModePersistence},
    context::{ContextEvent, ContextStore},
    error::{AshError, Result},
    shell::{ExecutionResult, ShellExecutor},
    stream::AgentStreamEvent,
};

const AGENT_CONTEXT_EVENT_LIMIT: usize = 24;
const AGENT_CONTEXT_EVENT_BODY_LIMIT: usize = 2_000;

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
        self.handle_line_stream(line, |_| Ok(()))
    }

    pub fn handle_line_stream(
        &mut self,
        line: &str,
        on_agent_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<SessionResponse> {
        let input = line.trim_end_matches(['\r', '\n']);
        if input.is_empty() {
            return Ok(SessionResponse::Empty);
        }
        if input == "\t" {
            return self.toggle_mode();
        }

        match self.mode {
            PromptMode::Agent => self.handle_agent_prompt(input, on_agent_event),
            PromptMode::Command => self.handle_command(input),
        }
    }

    fn handle_agent_prompt(
        &mut self,
        input: &str,
        on_agent_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<SessionResponse> {
        let recent_context = self.context.recent(AGENT_CONTEXT_EVENT_LIMIT)?;
        let agent_prompt = agent_prompt_with_context(input, &recent_context);
        self.context.record(ContextEvent::agent_prompt(input))?;
        let response = self.agent.respond_stream(&agent_prompt, on_agent_event)?;
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

fn agent_prompt_with_context(input: &str, events: &[ContextEvent]) -> String {
    if events.is_empty() {
        return input.to_owned();
    }

    let mut prompt = String::from(
        "You are ASH, an agentic shell. Use the recent ASH context below to resolve references like \"again\", \"that\", and \"same command\". Do not mention this context unless it is relevant.\n\n<ash_context>\n",
    );
    for event in events {
        prompt.push_str("- ");
        prompt.push_str(&event.kind);
        prompt.push_str(": ");
        prompt.push_str(&truncate_context_body(&event.body));
        prompt.push('\n');
    }
    prompt.push_str("</ash_context>\n\nCurrent user prompt:\n");
    prompt.push_str(input);
    prompt
}

fn truncate_context_body(body: &str) -> String {
    let mut truncated = String::new();
    for (index, character) in body.chars().enumerate() {
        if index == AGENT_CONTEXT_EVENT_BODY_LIMIT {
            truncated.push_str("...");
            break;
        }
        truncated.push(character);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use crate::{
        agent::{Agent, EchoAgent},
        config::{AshConfig, ModePersistence},
        context::InMemoryContextStore,
        error::Result,
        stream::AgentStreamEvent,
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

    #[test]
    fn agent_lines_stream_chunks_before_returning_final_response() {
        let mut session = AshSession::new(
            AshConfig::default(),
            InMemoryContextStore::default(),
            StreamingAgent,
            std::env::current_dir().expect("cwd"),
        );
        let mut chunks = Vec::new();

        let response = session
            .handle_line_stream("hello", |chunk| {
                chunks.push(chunk);
                Ok(())
            })
            .expect("agent response");

        assert_eq!(
            chunks,
            vec![
                AgentStreamEvent::AssistantText("agent: ".to_owned()),
                AgentStreamEvent::AssistantText("hello".to_owned()),
            ]
        );
        assert_eq!(response, SessionResponse::Agent("agent: hello".to_owned()));
    }

    #[test]
    fn agent_lines_include_recent_context_for_follow_up_prompts() {
        let mut session = AshSession::new(
            AshConfig::default(),
            InMemoryContextStore::default(),
            EchoAgent,
            std::env::current_dir().expect("cwd"),
        );

        session
            .handle_line("remember that git status was clean")
            .expect("first prompt");
        let response = session.handle_line("again").expect("follow up");

        let SessionResponse::Agent(text) = response else {
            panic!("expected agent response");
        };
        assert!(text.contains("<ash_context>"));
        assert!(text.contains("remember that git status was clean"));
        assert!(text.contains("Current user prompt:\nagain"));
    }

    struct StreamingAgent;

    impl Agent for StreamingAgent {
        fn respond(&mut self, prompt: &str) -> Result<String> {
            Ok(format!("agent: {prompt}"))
        }

        fn respond_stream(
            &mut self,
            prompt: &str,
            mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
        ) -> Result<String> {
            on_event(AgentStreamEvent::AssistantText("agent: ".to_owned()))?;
            on_event(AgentStreamEvent::AssistantText(prompt.to_owned()))?;
            Ok(format!("agent: {prompt}"))
        }
    }
}
