use std::{
    path::PathBuf,
    process::{Command, Stdio},
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
}

#[derive(Debug, Clone)]
pub struct CodexProvider {
    executable: PathBuf,
}

impl CodexProvider {
    pub fn discover() -> Result<Self> {
        let output = Command::new("which")
            .arg("codex")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(AshError::CodexNotFound);
        }

        let executable = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if executable.is_empty() {
            return Err(AshError::CodexNotFound);
        }

        Ok(Self {
            executable: PathBuf::from(executable),
        })
    }

    #[must_use]
    pub const fn new(executable: PathBuf) -> Self {
        Self { executable }
    }
}

impl Provider for CodexProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        let output = Command::new(&self.executable)
            .arg("exec")
            .arg("--ask-for-approval")
            .arg("untrusted")
            .arg("--sandbox")
            .arg("workspace-write")
            .arg("--cd")
            .arg(&request.cwd)
            .arg(&request.prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|source| AshError::ProcessSpawn {
                program: self.executable.display().to_string(),
                source,
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let text = if stdout.trim().is_empty() {
            stderr.trim().to_owned()
        } else {
            stdout.trim().to_owned()
        };

        Ok(ProviderResponse { text })
    }
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
