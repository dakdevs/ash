use std::{fs, path::Path};

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
            permissions: PermissionSet::secure_default(),
            plugins: Vec::new(),
            startup_commands: Vec::new(),
        }
    }
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

fn split_words(line: &str) -> std::result::Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in line.chars() {
        match (quote, character) {
            (None, '#') => break,
            (None, '"' | '\'') => quote = Some(character),
            (Some(active), character) if active == character => quote = None,
            (None, character) if character.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (_, character) => current.push(character),
        }
    }

    if quote.is_some() {
        return Err("unterminated quote".to_owned());
    }

    if !current.is_empty() {
        words.push(current);
    }

    Ok(words)
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
}
