use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    Wasm,
    Process,
}

impl PluginKind {
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        match value {
            "wasm" => Ok(Self::Wasm),
            "process" => Ok(Self::Process),
            other => Err(format!("unknown plugin kind `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    LocalPath(PathBuf),
    Git(String),
    Registry(String),
}

impl PluginSource {
    #[must_use]
    pub fn parse(location: &str) -> Self {
        let has_git_extension = std::path::Path::new(location)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("git"));

        if location.starts_with("git+") || has_git_extension {
            Self::Git(location.trim_start_matches("git+").to_owned())
        } else if location.starts_with('.')
            || location.starts_with('/')
            || location.starts_with('~')
        {
            Self::LocalPath(PathBuf::from(location))
        } else {
            Self::Registry(location.to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub kind: PluginKind,
    pub source: PluginSource,
    pub capabilities: Vec<String>,
}

impl PluginManifest {
    pub fn new(
        name: impl Into<String>,
        kind: PluginKind,
        source: PluginSource,
        capabilities: &[String],
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            source,
            capabilities: capabilities.to_vec(),
        }
    }

    #[must_use]
    pub fn provides_statusline(&self) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == "statusline")
    }
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    manifests: Vec<PluginManifest>,
}

impl PluginRegistry {
    #[must_use]
    pub fn from_manifests(manifests: Vec<PluginManifest>) -> Self {
        Self { manifests }
    }

    #[must_use]
    pub fn manifests(&self) -> &[PluginManifest] {
        &self.manifests
    }

    #[must_use]
    pub fn statusline_manifests(&self) -> Vec<&PluginManifest> {
        self.manifests
            .iter()
            .filter(|manifest| manifest.provides_statusline())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEvent {
    CommandExecuted { command: String, status: i32 },
    AgentPrompt { prompt: String },
    SessionCompacted,
}

#[cfg(test)]
mod tests {
    use super::{PluginKind, PluginManifest, PluginRegistry, PluginSource};

    #[test]
    fn registry_finds_plugins_with_statusline_capability() {
        let manifests = vec![
            PluginManifest::new(
                "prompt",
                PluginKind::Wasm,
                PluginSource::Registry("prompt".to_owned()),
                &["ui".to_owned()],
            ),
            PluginManifest::new(
                "gitline",
                PluginKind::Process,
                PluginSource::Registry("gitline".to_owned()),
                &["statusline".to_owned()],
            ),
        ];
        let registry = PluginRegistry::from_manifests(manifests);

        let statusline = registry.statusline_manifests();

        assert_eq!(statusline.len(), 1);
        assert_eq!(statusline[0].name, "gitline");
    }
}
