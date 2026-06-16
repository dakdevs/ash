use std::{
    fs,
    hash::Hasher,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use fnv::FnvHasher;
use serde::{Deserialize, Serialize};

use crate::{
    config::AiProviderConfig,
    error::{AshError, Result},
    providers::{Provider, ProviderRequest, ProviderResponse},
    stream::{AgentStreamEvent, TokenUsage},
};

const EMBEDDED_BRIDGE: &[u8] = include_bytes!(env!("ASH_EMBEDDED_ANTHROPIC_BRIDGE"));

#[derive(Debug, Clone)]
pub struct AnthropicAgentProvider {
    bridge: BridgeExecutable,
    model: Option<String>,
}

impl AnthropicAgentProvider {
    #[must_use]
    pub fn from_config(config: &AiProviderConfig) -> Self {
        Self {
            bridge: BridgeExecutable::default(),
            model: config.model.clone(),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_bridge(bridge: PathBuf, model: Option<String>) -> Self {
        Self {
            bridge: BridgeExecutable::Path(bridge),
            model,
        }
    }
}

impl Provider for AnthropicAgentProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.complete_stream(request, |_| Ok(()))
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let bridge = self.bridge.path()?;
        let mut child = Command::new(&bridge)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| AshError::ProcessSpawn {
                program: bridge.display().to_string(),
                source,
            })?;

        let bridge_request = AnthropicBridgeRequest {
            prompt: &request.prompt,
            cwd: &request.cwd,
            model: self.model.as_deref(),
        };
        {
            let mut stdin = child.stdin.take().ok_or_else(|| AshError::ProcessSpawn {
                program: bridge.display().to_string(),
                source: std::io::Error::other("failed to capture stdin"),
            })?;
            serde_json::to_writer(&mut stdin, &bridge_request)?;
            stdin.write_all(b"\n")?;
        }

        let stdout = child.stdout.take().ok_or_else(|| AshError::ProcessSpawn {
            program: bridge.display().to_string(),
            source: std::io::Error::other("failed to capture stdout"),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| AshError::ProcessSpawn {
            program: bridge.display().to_string(),
            source: std::io::Error::other("failed to capture stderr"),
        })?;

        let mut stream = AnthropicBridgeStream::default();
        for line in BufReader::new(stdout).lines() {
            stream.process_line(&line?, &mut on_event)?;
        }

        let status = child.wait().map_err(|source| AshError::ProcessWait {
            program: bridge.display().to_string(),
            source,
        })?;
        let stderr = read_to_string(stderr)?;
        if !status.success() {
            return Err(AshError::ProviderTransport(anthropic_error_message(
                stream.error.as_deref(),
                &stderr,
            )));
        }
        if let Some(error) = stream.error {
            return Err(AshError::ProviderTransport(anthropic_error_message(
                Some(&error),
                &stderr,
            )));
        }

        Ok(ProviderResponse { text: stream.text })
    }
}

#[derive(Debug, Clone)]
enum BridgeExecutable {
    Embedded { path: Option<PathBuf> },
    Path(PathBuf),
}

impl Default for BridgeExecutable {
    fn default() -> Self {
        std::env::var_os("ASH_ANTHROPIC_BRIDGE")
            .map(PathBuf::from)
            .map_or(Self::Embedded { path: None }, Self::Path)
    }
}

impl BridgeExecutable {
    fn path(&mut self) -> Result<PathBuf> {
        match self {
            Self::Path(path) => Ok(path.clone()),
            Self::Embedded { path } => {
                if let Some(path) = path {
                    return Ok(path.clone());
                }
                let extracted_path = extract_embedded_bridge()?;
                *path = Some(extracted_path.clone());
                Ok(extracted_path)
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnthropicBridgeRequest<'a> {
    prompt: &'a str,
    cwd: &'a Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBridgeEvent {
    Status {
        status: String,
    },
    AssistantText {
        text: String,
    },
    ToolStarted {
        command: String,
    },
    ToolOutput {
        output: String,
    },
    ToolCompleted {
        #[serde(default)]
        exit_code: Option<i64>,
    },
    Usage {
        input_tokens: i64,
        output_tokens: i64,
        #[serde(default)]
        cache_read_input_tokens: Option<i64>,
        #[serde(default)]
        cache_creation_input_tokens: Option<i64>,
    },
    Result {
        text: String,
    },
    Error {
        message: String,
        #[serde(default)]
        code: Option<String>,
    },
}

#[derive(Debug, Default)]
struct AnthropicBridgeStream {
    text: String,
    error: Option<String>,
}

impl AnthropicBridgeStream {
    fn process_line(
        &mut self,
        line: &str,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<()> {
        if line.trim().is_empty() {
            return Ok(());
        }

        match serde_json::from_str::<AnthropicBridgeEvent>(line)? {
            AnthropicBridgeEvent::Status { status } => {
                on_event(AgentStreamEvent::Status(status))?;
            }
            AnthropicBridgeEvent::AssistantText { text } => {
                self.text.push_str(&text);
                on_event(AgentStreamEvent::assistant_text(text))?;
            }
            AnthropicBridgeEvent::ToolStarted { command } => {
                on_event(AgentStreamEvent::ToolStarted { command })?;
            }
            AnthropicBridgeEvent::ToolOutput { output } => {
                on_event(AgentStreamEvent::ToolOutput(output))?;
            }
            AnthropicBridgeEvent::ToolCompleted { exit_code } => {
                on_event(AgentStreamEvent::ToolCompleted { exit_code })?;
            }
            AnthropicBridgeEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
            } => {
                on_event(AgentStreamEvent::Usage(TokenUsage {
                    input_tokens,
                    cached_input_tokens: cache_read_input_tokens.or(cache_creation_input_tokens),
                    output_tokens,
                    reasoning_output_tokens: None,
                }))?;
            }
            AnthropicBridgeEvent::Result { text } => {
                if self.text.is_empty() {
                    self.text = text;
                }
            }
            AnthropicBridgeEvent::Error { message, code } => {
                self.error = Some(match code {
                    Some(code) => format!("{code}: {message}"),
                    None => message,
                });
            }
        }

        Ok(())
    }
}

fn extract_embedded_bridge() -> Result<PathBuf> {
    let hash = fnv1a64(EMBEDDED_BRIDGE);
    let dir = std::env::temp_dir().join("ash-anthropic-agent");
    fs::create_dir_all(&dir)?;
    set_private_dir_permissions(&dir)?;

    let path = dir.join(executable_name(&format!("ash-anthropic-agent-{hash:016x}")));
    if path.exists() {
        return Ok(path);
    }

    let tmp_path = dir.join(format!(
        "{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ash-anthropic-agent"),
        std::process::id()
    ));
    fs::write(&tmp_path, EMBEDDED_BRIDGE)?;
    set_executable_permissions(&tmp_path)?;
    fs::rename(&tmp_path, &path)?;
    Ok(path)
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hasher = FnvHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn read_to_string(mut reader: impl std::io::Read) -> Result<String> {
    let mut output = String::new();
    reader.read_to_string(&mut output)?;
    Ok(output)
}

fn anthropic_error_message(error: Option<&str>, stderr: &str) -> String {
    let message = error
        .filter(|message| !message.trim().is_empty())
        .or_else(|| {
            let stderr = stderr.trim();
            (!stderr.is_empty()).then_some(stderr)
        })
        .unwrap_or("Claude Agent SDK bridge failed");

    if is_auth_error(message) {
        format!(
            "{message}. Claude authentication needs to be refreshed; run `claude` and log in, or check Claude Code auth with `/status`."
        )
    } else {
        message.to_owned()
    }
}

fn is_auth_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("auth")
        || message.contains("oauth")
        || message.contains("login")
        || message.contains("credential")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::{AnthropicAgentProvider, anthropic_error_message};
    use crate::{
        providers::{Provider, ProviderRequest},
        stream::{AgentStreamEvent, TokenUsage},
    };

    #[test]
    fn fake_bridge_events_stream_into_agent_events() {
        let dir = tempdir().expect("tempdir");
        let bridge = fake_bridge(dir.path(), true);
        let mut provider = AnthropicAgentProvider::with_bridge(bridge, Some("sonnet".to_owned()));
        let mut events = Vec::new();

        let response = provider
            .complete_stream(
                ProviderRequest {
                    prompt: "hello".to_owned(),
                    cwd: dir.path().to_path_buf(),
                },
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .expect("response");

        assert_eq!(response.text, "done");
        assert!(events.contains(&AgentStreamEvent::Status("started".to_owned())));
        assert!(events.contains(&AgentStreamEvent::AssistantText("done".to_owned())));
        assert!(events.contains(&AgentStreamEvent::ToolStarted {
            command: "git status --short".to_owned()
        }));
        assert!(events.contains(&AgentStreamEvent::ToolOutput("clean".to_owned())));
        assert!(events.contains(&AgentStreamEvent::ToolCompleted { exit_code: None }));
        assert!(events.contains(&AgentStreamEvent::Usage(TokenUsage {
            input_tokens: 7,
            cached_input_tokens: Some(3),
            output_tokens: 2,
            reasoning_output_tokens: None,
        })));
    }

    #[test]
    fn fake_bridge_auth_error_is_actionable() {
        let dir = tempdir().expect("tempdir");
        let bridge = fake_bridge(dir.path(), false);
        let mut provider = AnthropicAgentProvider::with_bridge(bridge, None);

        let error = provider
            .complete(ProviderRequest {
                prompt: "hello".to_owned(),
                cwd: dir.path().to_path_buf(),
            })
            .expect_err("error");

        assert!(error.to_string().contains("Claude authentication"));
    }

    #[test]
    fn auth_error_message_mentions_claude_login() {
        let message = anthropic_error_message(Some("authentication failed"), "");

        assert!(message.contains("Claude authentication"));
        assert!(message.contains("claude"));
    }

    fn fake_bridge(dir: &Path, success: bool) -> std::path::PathBuf {
        let bridge = dir.join("bridge");
        let script = if success {
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"status","status":"started"}'
printf '%s\n' '{"type":"tool_started","command":"git status --short"}'
printf '%s\n' '{"type":"tool_output","output":"clean"}'
printf '%s\n' '{"type":"tool_completed","exit_code":null}'
printf '%s\n' '{"type":"assistant_text","text":"done"}'
printf '%s\n' '{"type":"usage","input_tokens":7,"output_tokens":2,"cache_read_input_tokens":3}'
printf '%s\n' '{"type":"result","text":"done"}'
"#
        } else {
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"error","message":"authentication failed"}'
exit 1
"#
        };
        fs::write(&bridge, script).expect("bridge");
        make_executable(&bridge);
        bridge
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
