use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use base64::{
    Engine as _, alphabet,
    engine::{
        DecodePaddingMode,
        general_purpose::{GeneralPurpose, GeneralPurposeConfig},
    },
};
use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    error::{AshError, Result},
    providers::{Provider, ProviderRequest, ProviderResponse},
    stream::{AgentStreamEvent, TokenUsage},
};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_API_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";
const ISSUER: &str = "https://auth.openai.com";
const DEFAULT_MODEL: &str = "gpt-5.5";
const CODEX_INSTRUCTIONS: &str = "You are ASH, an agentic shell assistant running inside the user's terminal. Answer concisely. Use the shell tool when you need live workspace, git, filesystem, or command output. Do not claim you ran commands unless you used the tool.";
const MAX_TOOL_ROUNDS: usize = 4;
const AUTH_FILE_ENV: &str = "ASH_CODEX_AUTH_FILE";
const API_ENDPOINT_ENV: &str = "ASH_CODEX_API_ENDPOINT";
const ISSUER_ENV: &str = "ASH_CODEX_ISSUER";
const BASE64_URL: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone)]
pub struct CodexSubscriptionProvider {
    auth_path: PathBuf,
    endpoint: String,
    issuer: String,
    model: String,
    session_id: String,
    client: Client,
}

impl CodexSubscriptionProvider {
    pub fn discover() -> Result<Self> {
        let auth_path = configured_auth_path()?;
        if !auth_path.is_file() {
            return Err(AshError::CodexNotFound);
        }

        Self::from_auth_path(auth_path)
    }

    pub fn from_auth_path(auth_path: PathBuf) -> Result<Self> {
        let endpoint =
            std::env::var(API_ENDPOINT_ENV).unwrap_or_else(|_| CODEX_API_ENDPOINT.to_owned());
        let issuer = std::env::var(ISSUER_ENV).unwrap_or_else(|_| ISSUER.to_owned());
        Self::new(auth_path, endpoint, issuer, DEFAULT_MODEL.to_owned())
    }

    pub fn new(
        auth_path: PathBuf,
        endpoint: String,
        issuer: String,
        model: String,
    ) -> Result<Self> {
        Ok(Self {
            auth_path,
            endpoint,
            issuer,
            model,
            session_id: format!("ash-{}", std::process::id()),
            client: Client::builder().build().map_err(AshError::Http)?,
        })
    }

    #[must_use]
    pub fn auth_path(&self) -> &Path {
        &self.auth_path
    }
}

impl Provider for CodexSubscriptionProvider {
    fn complete(&mut self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.complete_stream(request, |_| Ok(()))
    }

    fn complete_stream(
        &mut self,
        request: ProviderRequest,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let mut auth = read_auth_cache(&self.auth_path)?;
        if auth.tokens.access_token.is_empty() {
            auth = self.refresh_auth(auth)?;
        }

        match self.send_streaming_request(&request, &auth, &mut on_event) {
            Ok(response) => Ok(response),
            Err(AshError::CodexAuthExpired) => {
                let refreshed = self.refresh_auth(auth)?;
                self.send_streaming_request(&request, &refreshed, on_event)
            }
            Err(error) => Err(error),
        }
    }
}

impl CodexSubscriptionProvider {
    fn send_streaming_request(
        &self,
        request: &ProviderRequest,
        auth: &CodexAuthCache,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let mut input = initial_input(&request.prompt);
        let mut text = String::new();

        for _ in 0..MAX_TOOL_ROUNDS {
            let body = codex_request_body(&self.model, &self.session_id, &input);
            let turn = self.send_codex_turn(&body, auth, &mut on_event)?;
            text.push_str(turn.text.trim());

            if turn.tool_calls.is_empty() {
                return Ok(ProviderResponse { text });
            }

            for tool_call in turn.tool_calls {
                input.push(tool_call.as_response_item());
                let output = run_shell_tool(&tool_call, &request.cwd, &mut on_event)?;
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_call.call_id,
                    "output": output,
                }));
            }
        }

        Err(AshError::ProviderTransport(format!(
            "Codex exceeded {MAX_TOOL_ROUNDS} native tool rounds"
        )))
    }

    fn send_codex_turn(
        &self,
        body: &Value,
        auth: &CodexAuthCache,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<CodexTurn> {
        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&auth.tokens.access_token)
            .header("ChatGPT-Account-Id", &auth.tokens.account_id)
            .header("originator", "ash")
            .header("session-id", &self.session_id)
            .header("User-Agent", ash_user_agent())
            .header("Accept", "text/event-stream")
            .json(body)
            .send()
            .map_err(AshError::Http)?;

        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(AshError::CodexAuthExpired);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|error| format!("failed to read error body: {error}"));
            return Err(AshError::ProviderTransport(format!(
                "Codex backend returned HTTP {status}: {}",
                body.trim()
            )));
        }

        let mut sse = CodexResponsesSse::default();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = response.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buffer[..read]);
            sse.push(&chunk, &mut on_event)?;
        }
        sse.finish(&mut on_event)?;

        Ok(CodexTurn {
            text: sse.agent_text().trim().to_owned(),
            tool_calls: sse.into_tool_calls(),
        })
    }

    fn refresh_auth(&self, auth: CodexAuthCache) -> Result<CodexAuthCache> {
        if auth.tokens.refresh_token.is_empty() {
            return Err(codex_auth_guidance());
        }

        let token_endpoint = format!("{}/oauth/token", self.issuer.trim_end_matches('/'));
        let response = self
            .client
            .post(token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", auth.tokens.refresh_token.as_str()),
                ("client_id", CLIENT_ID),
            ])
            .send()
            .map_err(AshError::Http)?;

        if !response.status().is_success() {
            return Err(codex_auth_guidance());
        }

        let refreshed: TokenResponse = response.json().map_err(AshError::Http)?;
        let mut next_auth = auth;
        next_auth.auth_mode = Some("chatgpt".to_owned());
        next_auth.tokens.access_token = refreshed.access;
        next_auth.tokens.refresh_token = refreshed.refresh;
        if let Some(id_token) = refreshed.id {
            next_auth.tokens.id_token = id_token;
        }
        if next_auth.tokens.account_id.is_empty()
            && let Some(account_id) = extract_account_id(&next_auth.tokens)
        {
            next_auth.tokens.account_id = account_id;
        }
        write_auth_cache(&self.auth_path, &next_auth)?;
        Ok(next_auth)
    }
}

fn configured_auth_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(AUTH_FILE_ENV) {
        return Ok(PathBuf::from(path));
    }

    let home = std::env::var_os("HOME").ok_or_else(|| {
        AshError::ProviderTransport("HOME is not set; cannot locate Codex auth cache".to_owned())
    })?;
    Ok(PathBuf::from(home).join(".codex/auth.json"))
}

fn initial_input(prompt: &str) -> Vec<Value> {
    vec![json!({
        "role": "user",
        "content": [
            {
                "type": "input_text",
                "text": prompt,
            }
        ],
    })]
}

fn codex_request_body(model: &str, session_id: &str, input: &[Value]) -> Value {
    json!({
        "model": model,
        "input": input,
        "instructions": CODEX_INSTRUCTIONS,
        "tools": [shell_tool_definition()],
        "tool_choice": "auto",
        "store": false,
        "stream": true,
        "prompt_cache_key": session_id,
        "include": ["reasoning.encrypted_content"],
        "reasoning": {
            "effort": "medium",
            "summary": "auto",
        },
    })
}

fn shell_tool_definition() -> Value {
    json!({
        "type": "function",
        "name": "shell",
        "description": "Run one shell command in the current ASH working directory and return stdout, stderr, and exit code.",
        "parameters": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to run.",
                },
            },
            "required": ["command"],
            "additionalProperties": false,
        },
        "strict": true,
    })
}

fn ash_user_agent() -> String {
    format!(
        "ash/{} ({} {}; {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    )
}

fn codex_auth_guidance() -> AshError {
    AshError::ProviderTransport(
        "Codex authentication needs to be refreshed. Run `ash auth codex`.".to_owned(),
    )
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexAuthCache {
    #[serde(rename = "OPENAI_API_KEY", default)]
    openai_api_key: Option<String>,
    #[serde(default)]
    auth_mode: Option<String>,
    tokens: CodexTokens,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexTokens {
    access_token: String,
    #[serde(default)]
    account_id: String,
    #[serde(default)]
    id_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    #[serde(rename = "id_token")]
    id: Option<String>,
    #[serde(rename = "access_token")]
    access: String,
    #[serde(rename = "refresh_token")]
    refresh: String,
}

fn read_auth_cache(path: &Path) -> Result<CodexAuthCache> {
    let source = fs::read_to_string(path)?;
    let auth: CodexAuthCache = serde_json::from_str(&source)?;
    if auth.tokens.access_token.is_empty() || auth.tokens.refresh_token.is_empty() {
        return Err(codex_auth_guidance());
    }
    Ok(auth)
}

fn write_auth_cache(path: &Path, auth: &CodexAuthCache) -> Result<()> {
    let content = serde_json::to_string_pretty(auth)?;
    fs::write(path, format!("{content}\n"))?;
    Ok(())
}

fn extract_account_id(tokens: &CodexTokens) -> Option<String> {
    parse_account_id_from_jwt(&tokens.id_token)
        .or_else(|| parse_account_id_from_jwt(&tokens.access_token))
}

fn parse_account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = BASE64_URL.decode(payload).ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("chatgpt_account_id")
        .or_else(|| {
            value
                .get("https://api.openai.com/auth")
                .and_then(|auth| auth.get("chatgpt_account_id"))
        })
        .or_else(|| {
            value
                .get("organizations")
                .and_then(Value::as_array)
                .and_then(|organizations| organizations.first())
                .and_then(|organization| organization.get("id"))
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexTurn {
    text: String,
    tool_calls: Vec<CodexToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

impl CodexToolCall {
    fn as_response_item(&self) -> Value {
        json!({
            "type": "function_call",
            "call_id": self.call_id,
            "name": self.name,
            "arguments": self.arguments,
        })
    }
}

fn run_shell_tool(
    tool_call: &CodexToolCall,
    cwd: &Path,
    mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
) -> Result<String> {
    if tool_call.name != "shell" {
        return Ok(format!("unsupported tool `{}`", tool_call.name));
    }

    let arguments = serde_json::from_str::<Value>(&tool_call.arguments)?;
    let Some(command) = arguments.get("command").and_then(Value::as_str) else {
        return Ok("missing required string argument `command`".to_owned());
    };

    on_event(AgentStreamEvent::ToolStarted {
        command: command.to_owned(),
    })?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let output = Command::new(shell)
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .output()
        .map_err(|source| AshError::ProcessSpawn {
            program: command.to_owned(),
            source,
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !combined.trim().is_empty() {
        on_event(AgentStreamEvent::ToolOutput(combined.clone()))?;
    }

    let exit_code = output.status.code().map(i64::from);
    on_event(AgentStreamEvent::ToolCompleted { exit_code })?;

    Ok(json!({
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": exit_code,
    })
    .to_string())
}

#[derive(Debug, Default)]
struct CodexResponsesSse {
    pending: String,
    agent_text: String,
    tool_calls: Vec<CodexToolCall>,
}

impl CodexResponsesSse {
    fn push(
        &mut self,
        chunk: &str,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<()> {
        self.pending.push_str(chunk);
        while let Some(frame_end) = self.pending.find("\n\n") {
            let frame = self.pending[..frame_end].to_owned();
            self.pending.drain(..frame_end + 2);
            self.process_frame(&frame, &mut on_event)?;
        }
        Ok(())
    }

    fn finish(&mut self, mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>) -> Result<()> {
        if !self.pending.trim().is_empty() {
            let frame = std::mem::take(&mut self.pending);
            self.process_frame(&frame, &mut on_event)?;
        }
        Ok(())
    }

    fn agent_text(&self) -> &str {
        &self.agent_text
    }

    fn into_tool_calls(self) -> Vec<CodexToolCall> {
        self.tool_calls
    }

    fn process_frame(
        &mut self,
        frame: &str,
        mut on_event: impl FnMut(AgentStreamEvent) -> Result<()>,
    ) -> Result<()> {
        let Some(data) = sse_frame_data(frame) else {
            return Ok(());
        };

        let Ok(event) = serde_json::from_str::<CodexResponseEvent>(&data) else {
            return Ok(());
        };

        for stream_event in codex_responses_events(&event) {
            if let AgentStreamEvent::AssistantText(text) = &stream_event {
                self.agent_text.push_str(text);
            }
            on_event(stream_event)?;
        }
        if let Some(tool_call) = codex_tool_call(&event) {
            self.tool_calls.push(tool_call);
        }
        Ok(())
    }
}

fn sse_frame_data(frame: &str) -> Option<String> {
    let mut data = String::new();
    for line in frame.lines().filter_map(|line| line.strip_prefix("data:")) {
        if !data.is_empty() {
            data.push('\n');
        }
        data.push_str(line.trim_start());
    }

    (!data.is_empty() && data != "[DONE]").then_some(data)
}

#[derive(Debug, Deserialize)]
struct CodexResponseEvent {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<String>,
    response: Option<CodexResponseEnvelope>,
    item: Option<CodexResponseItem>,
}

#[derive(Debug, Deserialize)]
struct CodexResponseEnvelope {
    usage: Option<CodexResponseUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CodexResponseItem {
    #[serde(rename = "function_call")]
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct CodexResponseUsage {
    input_tokens: i64,
    input_tokens_details: Option<CodexInputTokenDetails>,
    output_tokens: i64,
    output_tokens_details: Option<CodexOutputTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct CodexInputTokenDetails {
    cached_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CodexOutputTokenDetails {
    reasoning_tokens: Option<i64>,
}

impl From<&CodexResponseUsage> for AgentStreamEvent {
    fn from(usage: &CodexResponseUsage) -> Self {
        Self::Usage(TokenUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens: usage
                .input_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens),
            output_tokens: usage.output_tokens,
            reasoning_output_tokens: usage
                .output_tokens_details
                .as_ref()
                .and_then(|details| details.reasoning_tokens),
        })
    }
}

fn codex_tool_call(event: &CodexResponseEvent) -> Option<CodexToolCall> {
    if event.kind != "response.output_item.done" {
        return None;
    }

    let CodexResponseItem::FunctionCall {
        call_id,
        name,
        arguments,
    } = event.item.as_ref()?
    else {
        return None;
    };

    Some(CodexToolCall {
        call_id: call_id.clone(),
        name: name.clone(),
        arguments: arguments.clone(),
    })
}

fn codex_responses_events(event: &CodexResponseEvent) -> Vec<AgentStreamEvent> {
    match event.kind.as_str() {
        "response.output_text.delta" => event.delta.as_ref().map_or_else(Vec::new, |text| {
            vec![AgentStreamEvent::assistant_text(text.as_str())]
        }),
        "response.output_item.added" => output_item_added_events(event),
        "response.completed" | "response.done" => event
            .response
            .as_ref()
            .and_then(|response| response.usage.as_ref())
            .map(AgentStreamEvent::from)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn output_item_added_events(_event: &CodexResponseEvent) -> Vec<AgentStreamEvent> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        sync::mpsc,
        thread,
    };

    use base64::Engine as _;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        BASE64_URL, CodexResponsesSse, CodexSubscriptionProvider, codex_request_body,
        initial_input, parse_account_id_from_jwt,
    };
    use crate::{
        providers::{Provider, ProviderRequest},
        stream::{AgentStreamEvent, TokenUsage},
    };

    #[test]
    fn codex_responses_sse_streams_text_deltas_and_usage() {
        let mut stream = CodexResponsesSse::default();
        let mut events = Vec::new();
        stream
            .push(
                concat!(
                    "event: response.output_text.delta\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
                    "event: response.completed\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,",
                    "\"input_tokens_details\":{\"cached_tokens\":2},\"output_tokens\":3,",
                    "\"output_tokens_details\":{\"reasoning_tokens\":1}}}}\n\n",
                ),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .expect("push sse");

        assert_eq!(
            events,
            vec![
                AgentStreamEvent::AssistantText("hello".to_owned()),
                AgentStreamEvent::Usage(TokenUsage {
                    input_tokens: 5,
                    cached_input_tokens: Some(2),
                    output_tokens: 3,
                    reasoning_output_tokens: Some(1),
                }),
            ]
        );
        assert_eq!(stream.agent_text(), "hello");
    }

    #[test]
    fn codex_responses_sse_does_not_render_reasoning_summary_text() {
        let mut stream = CodexResponsesSse::default();
        let mut events = Vec::new();
        stream
            .push(
                concat!(
                    "event: response.output_item.added\n",
                    "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"reasoning\"}}\n\n",
                    "event: response.reasoning_summary_text.delta\n",
                    "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"private thought\"}\n\n",
                    "event: response.output_text.delta\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"visible answer\"}\n\n",
                ),
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .expect("push sse");

        assert_eq!(
            events,
            vec![AgentStreamEvent::AssistantText("visible answer".to_owned())]
        );
    }

    #[test]
    fn codex_request_body_matches_responses_streaming_shape() {
        let request = ProviderRequest {
            prompt: "say hi".to_owned(),
            cwd: PathBuf::from("/tmp/project"),
        };
        let input = initial_input(&request.prompt);
        let body = codex_request_body("gpt-5.5", "ash-test-session", &input);

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][0]["content"][0]["text"], "say hi");
        assert_eq!(body["prompt_cache_key"], "ash-test-session");
        assert_eq!(body["include"][0], "reasoning.encrypted_content");
    }

    #[test]
    fn codex_subscription_provider_posts_to_chatgpt_backend_and_streams_response() {
        let dir = tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");
        fs::write(
            &auth_path,
            r#"{"OPENAI_API_KEY":null,"auth_mode":"chatgpt","tokens":{"access_token":"access-test","account_id":"acc-test","id_token":"","refresh_token":"refresh-test"}}"#,
        )
        .expect("write auth");

        let (endpoint, request_rx, server) = start_sse_server(concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"native\"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\" stream\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n\n",
        ));
        let mut provider = CodexSubscriptionProvider::new(
            auth_path,
            endpoint,
            "http://127.0.0.1".to_owned(),
            "gpt-5.5".to_owned(),
        )
        .expect("provider");
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
            .expect("complete");

        server.join().expect("server");
        let captured = request_rx.recv().expect("captured request");
        assert!(
            captured
                .headers
                .contains("authorization: bearer access-test")
        );
        assert!(captured.headers.contains("chatgpt-account-id: acc-test"));
        assert_eq!(captured.body["model"], "gpt-5.5");
        assert_eq!(captured.body["stream"], true);
        assert_eq!(response.text, "native stream");
        assert_eq!(
            events,
            vec![
                AgentStreamEvent::AssistantText("native".to_owned()),
                AgentStreamEvent::AssistantText(" stream".to_owned()),
                AgentStreamEvent::Usage(TokenUsage {
                    input_tokens: 1,
                    cached_input_tokens: None,
                    output_tokens: 2,
                    reasoning_output_tokens: None,
                }),
            ]
        );
    }

    #[test]
    fn codex_subscription_provider_executes_shell_tool_and_continues_turn() {
        let dir = tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");
        fs::write(
            &auth_path,
            r#"{"OPENAI_API_KEY":null,"auth_mode":"chatgpt","tokens":{"access_token":"access-test","account_id":"acc-test","id_token":"","refresh_token":"refresh-test"}}"#,
        )
        .expect("write auth");

        let first = concat!(
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",",
            "\"call_id\":\"call_shell_1\",\"name\":\"shell\",",
            "\"arguments\":\"{\\\"command\\\":\\\"printf native-tool-output\\\"}\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
        );
        let second = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"saw tool output\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":4,\"output_tokens\":5}}}\n\n",
        );
        let (endpoint, request_rx, server) = start_sse_sequence_server(vec![first, second]);
        let mut provider = CodexSubscriptionProvider::new(
            auth_path,
            endpoint,
            "http://127.0.0.1".to_owned(),
            "gpt-5.5".to_owned(),
        )
        .expect("provider");
        let mut events = Vec::new();

        let response = provider
            .complete_stream(
                ProviderRequest {
                    prompt: "what is the output?".to_owned(),
                    cwd: dir.path().to_path_buf(),
                },
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .expect("complete");

        server.join().expect("server");
        let requests = request_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].body["tools"][0]["name"], "shell");
        let second_input = requests[1].body["input"].as_array().expect("input array");
        assert!(second_input.iter().any(|item| {
            item["type"] == "function_call_output"
                && item["output"]
                    .as_str()
                    .is_some_and(|output| output.contains("native-tool-output"))
        }));
        assert_eq!(response.text, "saw tool output");
        assert!(events.contains(&AgentStreamEvent::ToolStarted {
            command: "printf native-tool-output".to_owned()
        }));
        assert!(events.contains(&AgentStreamEvent::ToolOutput(
            "native-tool-output".to_owned()
        )));
        assert!(events.contains(&AgentStreamEvent::ToolCompleted { exit_code: Some(0) }));
        assert!(events.contains(&AgentStreamEvent::AssistantText(
            "saw tool output".to_owned()
        )));
    }

    #[test]
    fn jwt_account_id_extraction_matches_opencode_claim_order() {
        let payload = r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acc-nested"}}"#;
        let token = format!("header.{}.sig", base64_url_no_pad(payload.as_bytes()));

        assert_eq!(
            parse_account_id_from_jwt(&token),
            Some("acc-nested".to_owned())
        );
        assert_eq!(
            BASE64_URL
                .decode(base64_url_no_pad(b"hello"))
                .expect("decode"),
            b"hello"
        );
    }

    struct CapturedRequest {
        headers: String,
        body: Value,
    }

    fn start_sse_server(
        sse_body: &'static str,
    ) -> (
        String,
        mpsc::Receiver<CapturedRequest>,
        thread::JoinHandle<()>,
    ) {
        start_sse_sequence_server(vec![sse_body])
    }

    fn start_sse_sequence_server(
        sse_bodies: Vec<&'static str>,
    ) -> (
        String,
        mpsc::Receiver<CapturedRequest>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let endpoint = format!(
            "http://{}/codex/responses",
            listener.local_addr().expect("addr")
        );
        let (tx, rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for sse_body in sse_bodies {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 8192];
                loop {
                    let read = stream.read(&mut buffer).expect("read request");
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let request_text = String::from_utf8_lossy(&request);
                        let headers_end = request_text.find("\r\n\r\n").expect("headers end");
                        let headers = request_text[..headers_end].to_ascii_lowercase();
                        let content_length = parse_content_length(&headers);
                        let body_start = headers_end + 4;
                        while request.len() < body_start + content_length {
                            let read = stream.read(&mut buffer).expect("read body");
                            request.extend_from_slice(&buffer[..read]);
                        }
                        let body = serde_json::from_slice(
                            &request[body_start..body_start + content_length],
                        )
                        .expect("json body");
                        tx.send(CapturedRequest { headers, body }).expect("send");
                        break;
                    }
                }

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });
        (endpoint, rx, server)
    }

    fn parse_content_length(headers: &str) -> usize {
        headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse().ok())
            .expect("content-length")
    }

    fn base64_url_no_pad(input: &[u8]) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input)
    }
}
