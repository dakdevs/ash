use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Sender},
    thread,
};

use crate::error::{AshError, Result};

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
        mut on_chunk: impl FnMut(&str) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let response = self.complete(request)?;
        if !response.text.is_empty() {
            on_chunk(&response.text)?;
        }

        Ok(response)
    }
}

#[derive(Debug, Clone)]
pub enum AnyProvider {
    Codex(CodexProvider),
    Unimplemented(UnimplementedProvider),
}

impl Provider for AnyProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        match self {
            Self::Codex(provider) => provider.complete(request),
            Self::Unimplemented(provider) => provider.complete(request),
        }
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        on_chunk: impl FnMut(&str) -> Result<()>,
    ) -> Result<ProviderResponse> {
        match self {
            Self::Codex(provider) => provider.complete_stream(request, on_chunk),
            Self::Unimplemented(provider) => provider.complete_stream(request, on_chunk),
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
        CodexProvider, ProviderRequest, codex_exec_args, codex_response_text, stream_child_output,
    };

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
}

impl Provider for CodexProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.complete_stream(request, |_| Ok(()))
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        mut on_chunk: impl FnMut(&str) -> Result<()>,
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

        let output = stream_child_output(stdout, stderr, &mut on_chunk)?;
        child.wait().map_err(|source| AshError::ProcessWait {
            program: self.executable.display().to_string(),
            source,
        })?;

        let text = codex_response_text(&output.stdout, &output.stderr);
        if output.stdout.trim().is_empty() && !text.is_empty() {
            on_chunk(&text)?;
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
