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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEvent {
    CommandExecuted { command: String, status: i32 },
    AgentPrompt { prompt: String },
    SessionCompacted,
}
