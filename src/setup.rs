use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{AiProviderConfig, AshConfig, ProviderAuth},
    error::Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSetup {
    pub name: String,
    pub kind: String,
    pub env: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl ProviderSetup {
    #[must_use]
    pub fn into_config(self) -> AiProviderConfig {
        let auth = self
            .env
            .map_or_else(default_auth_for_kind(&self.kind), ProviderAuth::Env);
        let mut config = AiProviderConfig::new(self.name, self.kind, auth);
        config.base_url = self.base_url;
        config.model = self.model;
        config
    }
}

#[derive(Debug, Clone)]
pub struct AshrcEditor {
    path: PathBuf,
}

impl AshrcEditor {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn add_provider(&self, provider: &AiProviderConfig) -> Result<()> {
        let source = self.read_source()?;
        let mut lines = remove_provider_add(&source, &provider.name);
        lines.push(provider.as_ashrc_line());
        self.write_lines(&lines)
    }

    pub fn set_default_provider(&self, name: &str) -> Result<()> {
        let source = self.read_source()?;
        let mut lines = source
            .lines()
            .filter(|line| !is_provider_default_line(line))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        lines.push(format!("provider default {name}"));
        self.write_lines(&lines)
    }

    pub fn load_config(&self) -> Result<AshConfig> {
        AshConfig::load(Some(&self.path))
    }

    fn read_source(&self) -> Result<String> {
        if self.path.exists() {
            Ok(fs::read_to_string(&self.path)?)
        } else {
            Ok(String::new())
        }
    }

    fn write_lines(&self, lines: &[String]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut source = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        source.push('\n');
        fs::write(&self.path, source)?;
        Ok(())
    }
}

#[must_use]
pub fn default_env_for_kind(kind: &str) -> Option<String> {
    match kind {
        "openai" => Some("OPENAI_API_KEY".to_owned()),
        "openrouter" => Some("OPENROUTER_API_KEY".to_owned()),
        "vercel-ai-gateway" => Some("AI_GATEWAY_API_KEY".to_owned()),
        _ => None,
    }
}

#[must_use]
pub fn default_base_url_for_kind(kind: &str) -> Option<String> {
    match kind {
        "ollama" => Some("http://localhost:11434".to_owned()),
        _ => None,
    }
}

fn default_auth_for_kind(kind: &str) -> impl FnOnce() -> ProviderAuth + '_ {
    move || match kind {
        "codex" => ProviderAuth::CodexSubscription,
        "anthropic" => ProviderAuth::ClaudeCode,
        _ => default_env_for_kind(kind).map_or(ProviderAuth::None, ProviderAuth::Env),
    }
}

fn remove_provider_add(source: &str, name: &str) -> Vec<String> {
    source
        .lines()
        .filter(|line| !is_provider_add_line(line, name))
        .map(str::to_owned)
        .collect()
}

fn is_provider_add_line(line: &str, name: &str) -> bool {
    let mut words = line.split_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some("provider"), Some("add"), Some(provider_name)) if provider_name == name
    )
}

fn is_provider_default_line(line: &str) -> bool {
    let mut words = line.split_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some("provider"), Some("default"), Some(_))
    )
}

#[must_use]
pub fn doctor_lines(config: &AshConfig) -> Vec<String> {
    let mut lines = Vec::new();

    if config.providers.is_empty() {
        lines.push("no providers configured".to_owned());
        return lines;
    }

    for provider in config.providers.values() {
        match &provider.auth {
            ProviderAuth::Env(variable) if std::env::var_os(variable).is_none() => {
                lines.push(format!("{}: missing env {variable}", provider.name));
            }
            ProviderAuth::Env(variable) => {
                lines.push(format!("{}: env {variable} present", provider.name));
            }
            ProviderAuth::CodexSubscription => {
                lines.push(format!(
                    "{}: run `ash auth codex` to verify login",
                    provider.name
                ));
            }
            ProviderAuth::ClaudeCode => {
                lines.push(format!(
                    "{}: uses Claude Code authentication",
                    provider.name
                ));
            }
            ProviderAuth::None => lines.push(format!("{}: no auth required", provider.name)),
        }
    }

    if !config.providers.contains_key(&config.default_provider) {
        lines.push(format!(
            "default provider `{}` is not configured",
            config.default_provider
        ));
    }

    lines
}

#[must_use]
pub fn display_provider(provider: &AiProviderConfig, default_provider: &str) -> String {
    let marker = if provider.name == default_provider {
        "*"
    } else {
        " "
    };
    format!(
        "{marker} {} kind={} auth={}",
        provider.name,
        provider.kind,
        display_auth(&provider.auth)
    )
}

fn display_auth(auth: &ProviderAuth) -> String {
    match auth {
        ProviderAuth::None => "none".to_owned(),
        ProviderAuth::Env(variable) => format!("env:{variable}"),
        ProviderAuth::CodexSubscription => "codex-subscription".to_owned(),
        ProviderAuth::ClaudeCode => "claude-code".to_owned(),
    }
}

#[must_use]
pub fn default_ashrc_from_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join(".ashrc"))
}
