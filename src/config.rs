use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    error::{AshError, Result},
    permissions::{PermissionAction, PermissionRule, PermissionSet},
    plugins::{PluginKind, PluginManifest, PluginSource},
    session::PromptMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModePersistence {
    Persistent,
    OneShot,
}

impl ModePersistence {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "persistent" | "persist" => Ok(Self::Persistent),
            "one-shot" | "oneshot" | "one_command" => Ok(Self::OneShot),
            other => Err(AshError::AshrcParse {
                line: 0,
                message: format!("unknown command mode `{other}`"),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AshConfig {
    pub default_mode: PromptMode,
    pub command_mode: ModePersistence,
    pub default_provider: String,
    pub providers: BTreeMap<String, AiProviderConfig>,
    pub permissions: PermissionSet,
    pub plugins: Vec<PluginManifest>,
    pub startup_commands: Vec<String>,
}

impl Default for AshConfig {
    fn default() -> Self {
        Self {
            default_mode: PromptMode::Agent,
            command_mode: ModePersistence::Persistent,
            default_provider: "codex".to_owned(),
            providers: BTreeMap::new(),
            permissions: PermissionSet::secure_default(),
            plugins: Vec::new(),
            startup_commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub name: String,
    pub kind: String,
    pub auth: ProviderAuth,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl AiProviderConfig {
    pub fn new(name: impl Into<String>, kind: impl Into<String>, auth: ProviderAuth) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            auth,
            base_url: None,
            model: None,
        }
    }

    #[must_use]
    pub fn as_ashrc_line(&self) -> String {
        let mut parts = vec![
            "provider".to_owned(),
            "add".to_owned(),
            self.name.clone(),
            "kind".to_owned(),
            self.kind.clone(),
        ];

        match &self.auth {
            ProviderAuth::None => {}
            ProviderAuth::Env(variable) => {
                parts.push("env".to_owned());
                parts.push(variable.clone());
            }
            ProviderAuth::CodexSubscription => {
                parts.push("auth".to_owned());
                parts.push("codex-subscription".to_owned());
            }
            ProviderAuth::ClaudeCode => {
                parts.push("auth".to_owned());
                parts.push("claude-code".to_owned());
            }
        }

        if let Some(base_url) = &self.base_url {
            parts.push("base-url".to_owned());
            parts.push(base_url.clone());
        }

        if let Some(model) = &self.model {
            parts.push("model".to_owned());
            parts.push(model.clone());
        }

        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAuth {
    None,
    Env(String),
    CodexSubscription,
    ClaudeCode,
}

impl AshConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(path) if path.exists() => Self::from_file(path),
            Some(_) | None => Ok(Self::default()),
        }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)?;
        Self::from_ashrc(&source)
    }

    pub fn from_ashrc(source: &str) -> Result<Self> {
        AshrcEvaluator::default().evaluate(source)
    }
}

#[derive(Default)]
struct AshrcEvaluator {
    config: AshConfig,
}

impl AshrcEvaluator {
    fn evaluate(mut self, source: &str) -> Result<AshConfig> {
        for (index, raw_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            self.evaluate_line(line_number, line)?;
        }

        Ok(self.config)
    }

    fn evaluate_line(&mut self, line_number: usize, line: &str) -> Result<()> {
        let parts = split_words(line).map_err(|message| AshError::AshrcParse {
            line: line_number,
            message,
        })?;

        match parts.as_slice() {
            [command, key, value] if command == "set" => self.set_value(line_number, key, value),
            [command, subcommand, value] if command == "provider" && subcommand == "default" => {
                self.config.default_provider.clone_from(value);
                Ok(())
            }
            [command, subcommand, name, rest @ ..]
                if command == "provider" && subcommand == "add" =>
            {
                let provider = parse_provider_add(line_number, name, rest)?;
                self.config
                    .providers
                    .insert(provider.name.clone(), provider);
                Ok(())
            }
            [command, tool, pattern, action] if command == "permission" => {
                let action =
                    PermissionAction::parse(action).map_err(|err| AshError::AshrcParse {
                        line: line_number,
                        message: err.to_string(),
                    })?;
                self.config
                    .permissions
                    .set_rule(tool, PermissionRule::new(pattern, action));
                Ok(())
            }
            [command, kind, name, location, capabilities @ ..] if command == "plugin" => {
                let kind = PluginKind::parse(kind).map_err(|message| AshError::AshrcParse {
                    line: line_number,
                    message,
                })?;
                let source = PluginSource::parse(location);
                let manifest = PluginManifest::new(name, kind, source, capabilities);
                self.config.plugins.push(manifest);
                Ok(())
            }
            _ => {
                self.config.startup_commands.push(line.to_owned());
                Ok(())
            }
        }
    }

    fn set_value(&mut self, line_number: usize, key: &str, value: &str) -> Result<()> {
        match key {
            "default_mode" => {
                self.config.default_mode =
                    PromptMode::parse(value).map_err(|err| AshError::AshrcParse {
                        line: line_number,
                        message: err.to_string(),
                    })?;
                Ok(())
            }
            "command_mode" => {
                self.config.command_mode =
                    ModePersistence::parse(value).map_err(|err| AshError::AshrcParse {
                        line: line_number,
                        message: err.to_string(),
                    })?;
                Ok(())
            }
            other => Err(AshError::AshrcParse {
                line: line_number,
                message: format!("unknown setting `{other}`"),
            }),
        }
    }
}

fn parse_provider_add(
    line_number: usize,
    name: &str,
    tokens: &[String],
) -> Result<AiProviderConfig> {
    let mut kind = None;
    let mut auth = ProviderAuth::None;
    let mut base_url = None;
    let mut model = None;
    let mut index = 0;

    while index < tokens.len() {
        let key = tokens[index].as_str();
        let value = tokens.get(index + 1).ok_or_else(|| AshError::AshrcParse {
            line: line_number,
            message: format!("provider add `{name}` is missing a value for `{key}`"),
        })?;

        match key {
            "kind" => kind = Some(value.clone()),
            "env" => auth = ProviderAuth::Env(value.clone()),
            "auth" if value == "codex-subscription" => auth = ProviderAuth::CodexSubscription,
            "auth" if value == "claude-code" => auth = ProviderAuth::ClaudeCode,
            "auth" if value == "none" => auth = ProviderAuth::None,
            "auth" => {
                return Err(AshError::AshrcParse {
                    line: line_number,
                    message: format!("unknown provider auth `{value}`"),
                });
            }
            "base-url" => base_url = Some(value.clone()),
            "model" => model = Some(value.clone()),
            other => {
                return Err(AshError::AshrcParse {
                    line: line_number,
                    message: format!("unknown provider field `{other}`"),
                });
            }
        }

        index += 2;
    }

    let kind = kind.unwrap_or_else(|| name.to_owned());
    Ok(AiProviderConfig {
        name: name.to_owned(),
        kind,
        auth,
        base_url,
        model,
    })
}

fn split_words(line: &str) -> std::result::Result<Vec<String>, String> {
    shell_words::split(line).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{
        permissions::PermissionAction,
        plugins::{PluginKind, PluginSource},
        session::PromptMode,
    };

    use super::{AshConfig, ModePersistence};

    #[test]
    fn parses_declarative_ashrc_forms() {
        let config = AshConfig::from_ashrc(
            r#"
            # ASH startup
            set default_mode command
            set command_mode one-shot
            provider default codex
            permission bash "git status*" allow
            plugin wasm prompt ~/.config/ash/plugins/prompt.wasm ui prompt
            echo startup
            "#,
        )
        .expect("config");

        assert_eq!(config.default_mode, PromptMode::Command);
        assert_eq!(config.command_mode, ModePersistence::OneShot);
        assert_eq!(config.default_provider, "codex");
        assert!(config.providers.is_empty());
        assert_eq!(
            config.permissions.resolve("bash", "git status --short"),
            PermissionAction::Allow
        );
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].name, "prompt");
        assert_eq!(config.plugins[0].kind, PluginKind::Wasm);
        assert!(matches!(
            config.plugins[0].source,
            PluginSource::LocalPath(_)
        ));
        assert_eq!(config.startup_commands, vec!["echo startup"]);
    }

    #[test]
    fn parses_provider_add_forms() {
        let config = AshConfig::from_ashrc(concat!(
            "provider add openai kind openai env OPENAI_API_KEY model gpt-5\n",
            "provider add anthropic kind anthropic auth claude-code\n",
        ))
        .expect("config");

        let provider = config.providers.get("openai").expect("openai provider");
        assert_eq!(provider.kind, "openai");
        assert_eq!(
            provider.auth,
            super::ProviderAuth::Env("OPENAI_API_KEY".to_owned())
        );
        assert_eq!(provider.model.as_deref(), Some("gpt-5"));

        let provider = config
            .providers
            .get("anthropic")
            .expect("anthropic provider");
        assert_eq!(provider.kind, "anthropic");
        assert_eq!(provider.auth, super::ProviderAuth::ClaudeCode);
        assert_eq!(
            provider.as_ashrc_line(),
            "provider add anthropic kind anthropic auth claude-code"
        );
    }
}
