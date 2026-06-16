use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Sender},
    thread,
};

use serde_json::Value;

use crate::{
    codex_native::CodexSubscriptionProvider,
    error::{AshError, Result},
    stream::{AgentStreamEvent, TokenUsage},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    Codex,
    OpenAi,
    OpenRouter,
    VercelGateway,
    Anthropic,
    Ollama,
    OpenAiCompatible { base_url: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub prompt: String,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    pub text: String,
}

pub trait Provider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse>;

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let response = self.complete(request)?;
        if !response.text.is_empty() {
            on_event(AgentStreamEvent::assistant_text(&response.text))?;
        }

        Ok(response)
    }
}

#[derive(Debug, Clone)]
pub enum AnyProvider {
    CodexSubscription(CodexSubscriptionProvider),
    Codex(CodexProvider),
    Unimplemented(UnimplementedProvider),
}

impl Provider for AnyProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        match self {
            Self::CodexSubscription(provider) => provider.complete(request),
            Self::Codex(provider) => provider.complete(request),
            Self::Unimplemented(provider) => provider.complete(request),
        }
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        match self {
            Self::CodexSubscription(provider) => provider.complete_stream(request, on_event),
            Self::Codex(provider) => provider.complete_stream(request, on_event),
            Self::Unimplemented(provider) => provider.complete_stream(request, on_event),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodexProvider {
    executable: PathBuf,
}

impl CodexProvider {
    pub fn discover() -> Result<Self> {
        discover_codex_executable().map(Self::new)
    }

    #[must_use]
    pub const fn new(executable: PathBuf) -> Self {
        Self { executable }
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

fn discover_codex_executable() -> Result<PathBuf> {
    if let Ok(output) = Command::new("which")
        .arg("codex")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        && output.status.success()
    {
        let executable = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !executable.is_empty() {
            return Ok(PathBuf::from(executable));
        }
    }

    common_codex_paths()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or(AshError::CodexNotFound)
}

fn common_codex_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/opt/homebrew/bin/codex"),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join(".local/bin/codex"));
        paths.push(home.join(".cargo/bin/codex"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf};

    use super::{
        CodexJsonStream, CodexProvider, ProviderRequest, codex_exec_args, codex_response_text,
        codex_stream_line_events, stream_child_output,
    };
    use crate::stream::{AgentStreamEvent, TokenUsage};

    #[test]
    fn codex_provider_exposes_executable_path() {
        let provider = CodexProvider::new(PathBuf::from("/tmp/codex"));

        assert_eq!(provider.executable(), PathBuf::from("/tmp/codex"));
    }

    #[test]
    fn codex_exec_args_use_supported_exec_options() {
        let request = ProviderRequest {
            prompt: "hello".to_owned(),
            cwd: PathBuf::from("/tmp/project"),
        };

        assert_eq!(
            codex_exec_args(&request),
            vec![
                "exec",
                "--sandbox",
                "workspace-write",
                "--cd",
                "/tmp/project",
                "--skip-git-repo-check",
                "--color",
                "never",
                "--json",
                "hello",
            ]
        );
    }

    #[test]
    fn codex_auth_failures_are_mapped_to_setup_guidance() {
        let text = codex_response_text(
            "",
            "ERROR: Your access token could not be refreshed because your refresh token was revoked.",
        );

        assert_eq!(
            text,
            "Codex authentication needs to be refreshed. Run `ash auth codex`."
        );
    }

    #[test]
    fn child_stdout_is_streamed_and_collected() {
        let stdout = Cursor::new(Vec::from("hello ".as_bytes()));
        let stderr = Cursor::new(Vec::from("diagnostic".as_bytes()));
        let mut chunks = Vec::new();

        let output = stream_child_output(stdout, stderr, |chunk| {
            chunks.push(chunk.to_owned());
            Ok(())
        })
        .expect("stream output");

        assert_eq!(chunks, vec!["hello "]);
        assert_eq!(output.stdout, "hello ");
        assert_eq!(output.stderr, "diagnostic");
    }

    #[test]
    fn codex_json_lines_render_tool_events_and_agent_text() {
        assert_eq!(
            codex_stream_line_events(
                r#"{"type":"item.started","item":{"type":"command_execution","command":"/bin/zsh -lc 'git status --short'","status":"in_progress"}}"#
            ),
            vec![AgentStreamEvent::ToolStarted {
                command: "/bin/zsh -lc 'git status --short'".to_owned()
            }]
        );
        assert_eq!(
            codex_stream_line_events(
                r#"{"type":"item.completed","item":{"type":"command_execution","command":"/bin/zsh -lc 'git status --short'","exit_code":0,"status":"completed"}}"#
            ),
            vec![AgentStreamEvent::ToolCompleted { exit_code: Some(0) }]
        );
        assert_eq!(
            codex_stream_line_events(
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"Worktree is clean."}}"#
            ),
            vec![AgentStreamEvent::AssistantText(
                "Worktree is clean.".to_owned()
            )]
        );
    }

    #[test]
    fn codex_json_lines_render_tool_output_and_usage_separately() {
        assert_eq!(
            codex_stream_line_events(
                r#"{"type":"item.completed","item":{"type":"command_execution","command":"git status","aggregated_output":" M src/ui.rs\n","exit_code":0,"status":"completed"}}"#
            ),
            vec![
                AgentStreamEvent::ToolOutput(" M src/ui.rs\n".to_owned()),
                AgentStreamEvent::ToolCompleted { exit_code: Some(0) },
            ]
        );
        assert_eq!(
            codex_stream_line_events(
                r#"{"type":"turn.completed","usage":{"input_tokens":41454,"cached_input_tokens":17792,"output_tokens":146,"reasoning_output_tokens":22}}"#
            ),
            vec![AgentStreamEvent::Usage(TokenUsage {
                input_tokens: 41454,
                cached_input_tokens: Some(17792),
                output_tokens: 146,
                reasoning_output_tokens: Some(22),
            })]
        );
    }

    #[test]
    fn codex_json_lines_accept_agent_message_delta_shapes() {
        assert_eq!(
            codex_stream_line_events(
                r#"{"method":"item/agentMessage/delta","params":{"delta":"hello "}}"#
            ),
            vec![AgentStreamEvent::AssistantText("hello ".to_owned())]
        );
        assert_eq!(
            codex_stream_line_events(r#"{"type":"agent_message.delta","delta":"world"}"#),
            vec![AgentStreamEvent::AssistantText("world".to_owned())]
        );
    }

    #[test]
    fn codex_json_stream_handles_split_lines() {
        let mut stream = CodexJsonStream::default();
        let mut chunks = Vec::new();

        stream
            .push(
                "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",",
                |event| {
                    chunks.push(event);
                    Ok(())
                },
            )
            .expect("first chunk");
        stream
            .push("\"text\":\"Hello\"}}\n", |event| {
                chunks.push(event);
                Ok(())
            })
            .expect("second chunk");

        assert_eq!(
            chunks,
            vec![AgentStreamEvent::AssistantText("Hello".to_owned())]
        );
        assert_eq!(stream.agent_text(), "Hello");
    }
}

impl Provider for CodexProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.complete_stream(request, |_| Ok(()))
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let args = codex_exec_args(&request);
        let mut child = Command::new(&self.executable)
            .args(&args)
            .stdin(codex_stdin())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| AshError::ProcessSpawn {
                program: self.executable.display().to_string(),
                source,
            })?;

        let stdout = child.stdout.take().ok_or_else(|| AshError::ProcessSpawn {
            program: self.executable.display().to_string(),
            source: std::io::Error::other("failed to capture stdout"),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| AshError::ProcessSpawn {
            program: self.executable.display().to_string(),
            source: std::io::Error::other("failed to capture stderr"),
        })?;

        let mut json_stream = CodexJsonStream::default();
        let output = stream_child_output(stdout, stderr, |chunk| {
            json_stream.push(chunk, &mut on_event)
        })?;
        child.wait().map_err(|source| AshError::ProcessWait {
            program: self.executable.display().to_string(),
            source,
        })?;
        json_stream.finish(&mut on_event)?;

        let text = json_stream.agent_text().trim().to_owned();
        let text = if text.is_empty() {
            codex_response_text(&output.stdout, &output.stderr)
        } else {
            text
        };
        if json_stream.agent_text().trim().is_empty() && !text.is_empty() {
            on_event(AgentStreamEvent::assistant_text(&text))?;
        }

        Ok(ProviderResponse { text })
    }
}

fn codex_stdin() -> Stdio {
    Stdio::null()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ChildOutput {
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
enum ChildOutputEvent {
    Stdout(std::io::Result<String>),
    Stderr(std::io::Result<String>),
}

#[derive(Clone, Copy)]
enum ChildOutputStream {
    Stdout,
    Stderr,
}

impl ChildOutputStream {
    fn event(self, chunk: std::io::Result<String>) -> ChildOutputEvent {
        match self {
            Self::Stdout => ChildOutputEvent::Stdout(chunk),
            Self::Stderr => ChildOutputEvent::Stderr(chunk),
        }
    }
}

fn stream_child_output(
    stdout: impl Read + Send + 'static,
    stderr: impl Read + Send + 'static,
    mut on_stdout: impl FnMut(&str) -> Result<()>,
) -> Result<ChildOutput> {
    let (sender, receiver) = mpsc::channel();
    let stdout_thread = spawn_output_reader(stdout, ChildOutputStream::Stdout, sender.clone());
    let stderr_thread = spawn_output_reader(stderr, ChildOutputStream::Stderr, sender);

    let mut output = ChildOutput::default();
    for event in receiver {
        match event {
            ChildOutputEvent::Stdout(Ok(chunk)) => {
                on_stdout(&chunk)?;
                output.stdout.push_str(&chunk);
            }
            ChildOutputEvent::Stderr(Ok(chunk)) => {
                output.stderr.push_str(&chunk);
            }
            ChildOutputEvent::Stdout(Err(source)) | ChildOutputEvent::Stderr(Err(source)) => {
                return Err(AshError::Io(source));
            }
        }
    }

    join_output_reader(stdout_thread)?;
    join_output_reader(stderr_thread)?;

    Ok(output)
}

fn spawn_output_reader(
    mut reader: impl Read + Send + 'static,
    stream: ChildOutputStream,
    sender: Sender<ChildOutputEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            let event = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let chunk = String::from_utf8_lossy(&buffer[..read]).into_owned();
                    stream.event(Ok(chunk))
                }
                Err(source) => stream.event(Err(source)),
            };

            if sender.send(event).is_err() {
                break;
            }
        }
    })
}

fn join_output_reader(handle: thread::JoinHandle<()>) -> Result<()> {
    handle
        .join()
        .map_err(|_| AshError::Io(std::io::Error::other("output reader thread panicked")))
}

#[derive(Debug, Default)]
struct CodexJsonStream {
    pending_line: String,
    agent_text: String,
}

impl CodexJsonStream {
    fn push(
        &mut self,
        chunk: &str,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<()> {
        self.pending_line.push_str(chunk);
        while let Some(newline) = self.pending_line.find('\n') {
            let line = self.pending_line[..newline].to_owned();
            self.pending_line.drain(..=newline);
            self.process_line(&line, &mut on_event)?;
        }
        Ok(())
    }

    fn finish(&mut self, mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>) -> Result<()> {
        if !self.pending_line.trim().is_empty() {
            let line = std::mem::take(&mut self.pending_line);
            self.process_line(&line, &mut on_event)?;
        }
        Ok(())
    }

    fn agent_text(&self) -> &str {
        &self.agent_text
    }

    fn process_line(
        &mut self,
        line: &str,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<()> {
        for event in codex_stream_line_events(line) {
            if let AgentStreamEvent::AssistantText(text) = &event {
                self.agent_text.push_str(text);
            }
            on_event(event)?;
        }
        Ok(())
    }
}

fn codex_stream_line_events(line: &str) -> Vec<AgentStreamEvent> {
    let Ok(value) = serde_json::from_str(line) else {
        return Vec::new();
    };
    if let Some(event) = codex_json_rpc_delta_event(&value) {
        return vec![event];
    }

    match value.get("type").and_then(Value::as_str) {
        Some("turn.started") => vec![AgentStreamEvent::Status("started".to_owned())],
        Some("agent_message.delta" | "item.agent_message.delta") => value
            .get("delta")
            .or_else(|| value.get("text"))
            .and_then(Value::as_str)
            .map_or_else(Vec::new, |text| {
                vec![AgentStreamEvent::assistant_text(text)]
            }),
        Some("item.started") => value
            .get("item")
            .and_then(codex_item_started_chunk)
            .into_iter()
            .collect(),
        Some("item.completed") => value
            .get("item")
            .map_or_else(Vec::new, codex_item_completed_chunks),
        Some("turn.completed") => value
            .get("usage")
            .and_then(codex_turn_completed_chunk)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn codex_json_rpc_delta_event(value: &Value) -> Option<AgentStreamEvent> {
    match value.get("method")?.as_str()? {
        "item/agentMessage/delta" => value
            .get("params")
            .and_then(|params| params.get("delta").or_else(|| params.get("text")))
            .and_then(Value::as_str)
            .map(AgentStreamEvent::assistant_text),
        _ => None,
    }
}

fn codex_item_started_chunk(item: &Value) -> Option<AgentStreamEvent> {
    match item.get("type")?.as_str()? {
        "command_execution" => {
            let command = item.get("command")?.as_str()?;
            Some(AgentStreamEvent::ToolStarted {
                command: command.to_owned(),
            })
        }
        _ => None,
    }
}

fn codex_item_completed_chunks(item: &Value) -> Vec<AgentStreamEvent> {
    match item.get("type").and_then(Value::as_str) {
        Some("agent_message") => item
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(Vec::new, |text| {
                vec![AgentStreamEvent::assistant_text(text)]
            }),
        Some("command_execution") => codex_command_completed_chunks(item),
        _ => Vec::new(),
    }
}

fn codex_command_completed_chunks(item: &Value) -> Vec<AgentStreamEvent> {
    let mut events = Vec::new();
    if let Some(output) = item.get("aggregated_output").and_then(Value::as_str)
        && !output.trim().is_empty()
    {
        events.push(AgentStreamEvent::ToolOutput(output.to_owned()));
    }

    let exit_code = item.get("exit_code").and_then(Value::as_i64);
    events.push(AgentStreamEvent::ToolCompleted { exit_code });
    events
}

fn codex_turn_completed_chunk(usage: &Value) -> Option<AgentStreamEvent> {
    let input = usage.get("input_tokens").and_then(Value::as_i64)?;
    let output = usage.get("output_tokens").and_then(Value::as_i64)?;
    Some(AgentStreamEvent::Usage(TokenUsage {
        input_tokens: input,
        cached_input_tokens: usage.get("cached_input_tokens").and_then(Value::as_i64),
        output_tokens: output,
        reasoning_output_tokens: usage.get("reasoning_output_tokens").and_then(Value::as_i64),
    }))
}

fn codex_response_text(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stdout}\n{stderr}");
    if is_codex_auth_failure(&combined) {
        return "Codex authentication needs to be refreshed. Run `ash auth codex`.".to_owned();
    }

    if stdout.trim().is_empty() {
        stderr.trim().to_owned()
    } else {
        stdout.trim().to_owned()
    }
}

fn is_codex_auth_failure(output: &str) -> bool {
    output.contains("token_invalidated")
        || output.contains("refresh_token_invalidated")
        || output.contains("refresh token was revoked")
        || output.contains("Please log out and sign in again")
        || output.contains("Please try signing in again")
}

fn codex_exec_args(request: &ProviderRequest) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "--sandbox".to_owned(),
        "workspace-write".to_owned(),
        "--cd".to_owned(),
        request.cwd.display().to_string(),
        "--skip-git-repo-check".to_owned(),
        "--color".to_owned(),
        "never".to_owned(),
        "--json".to_owned(),
        request.prompt.clone(),
    ]
}

#[derive(Debug, Clone)]
pub struct UnimplementedProvider {
    name: String,
}

impl UnimplementedProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Provider for UnimplementedProvider {
    fn complete(&mut self, _request: ProviderRequest) -> Result<ProviderResponse> {
        Err(AshError::ProviderNotConfigured(self.name.clone()))
    }
}
