use std::{fs, path::Path, process::Command};

use crate::session::PromptMode;

#[derive(Debug, Clone, Copy)]
pub struct StatusLineContext<'a> {
    pub mode: PromptMode,
    pub provider: &'a str,
    pub cwd: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSegment {
    pub name: String,
    pub value: String,
}

impl StatusSegment {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    #[must_use]
    pub fn render(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

pub trait StatusLinePlugin: Send + Sync {
    fn segment(&self, context: &StatusLineContext<'_>) -> Option<StatusSegment>;
}

pub struct StatusLine {
    plugins: Vec<Box<dyn StatusLinePlugin>>,
}

impl Default for StatusLine {
    fn default() -> Self {
        Self::native()
    }
}

impl StatusLine {
    #[must_use]
    pub fn native() -> Self {
        Self {
            plugins: vec![
                Box::new(PwdStatusPlugin),
                Box::new(GitStatusPlugin),
                Box::new(NodeVersionStatusPlugin),
                Box::new(RustVersionStatusPlugin),
                Box::new(BatteryStatusPlugin),
            ],
        }
    }

    #[must_use]
    pub const fn from_plugins(plugins: Vec<Box<dyn StatusLinePlugin>>) -> Self {
        Self { plugins }
    }

    #[must_use]
    pub fn render(&self, context: &StatusLineContext<'_>) -> String {
        let mut segments = vec![
            StatusSegment::new("mode", context.mode.prompt()),
            StatusSegment::new("provider", context.provider),
        ];
        segments.extend(
            self.plugins
                .iter()
                .filter_map(|plugin| plugin.segment(context)),
        );
        segments
            .into_iter()
            .map(|segment| segment.render())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PwdStatusPlugin;

impl StatusLinePlugin for PwdStatusPlugin {
    fn segment(&self, context: &StatusLineContext<'_>) -> Option<StatusSegment> {
        Some(StatusSegment::new("pwd", context.cwd.display().to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GitStatusPlugin;

impl StatusLinePlugin for GitStatusPlugin {
    fn segment(&self, context: &StatusLineContext<'_>) -> Option<StatusSegment> {
        let output = Command::new("git")
            .arg("-C")
            .arg(context.cwd)
            .args(["status", "--short", "--branch"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        parse_git_status(&text).map(|value| StatusSegment::new("git", value))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NodeVersionStatusPlugin;

impl StatusLinePlugin for NodeVersionStatusPlugin {
    fn segment(&self, _context: &StatusLineContext<'_>) -> Option<StatusSegment> {
        command_first_line("node", &["--version"])
            .and_then(|line| parse_node_version(&line))
            .map(|version| StatusSegment::new("node", version))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RustVersionStatusPlugin;

impl StatusLinePlugin for RustVersionStatusPlugin {
    fn segment(&self, _context: &StatusLineContext<'_>) -> Option<StatusSegment> {
        command_first_line("rustc", &["--version"])
            .and_then(|line| parse_rust_version(&line))
            .map(|version| StatusSegment::new("rust", version))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryStatusPlugin;

impl StatusLinePlugin for BatteryStatusPlugin {
    fn segment(&self, _context: &StatusLineContext<'_>) -> Option<StatusSegment> {
        battery_percent().map(|percent| StatusSegment::new("battery", format!("{percent}%")))
    }
}

fn command_first_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_git_status(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let branch = lines
        .next()
        .and_then(|line| line.strip_prefix("## "))
        .map(|line| line.split("...").next().unwrap_or(line))
        .map(str::trim)
        .filter(|line| !line.is_empty())?;
    let changed = lines.count();
    let state = if changed == 0 {
        "clean".to_owned()
    } else {
        format!("{changed} changed")
    };
    Some(format!("{branch} {state}"))
}

fn parse_node_version(line: &str) -> Option<String> {
    line.strip_prefix('v')
        .filter(|version| !version.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_rust_version(line: &str) -> Option<String> {
    line.strip_prefix("rustc ")
        .and_then(|rest| rest.split_whitespace().next())
        .filter(|version| !version.is_empty())
        .map(ToOwned::to_owned)
}

fn battery_percent() -> Option<u8> {
    macos_battery_percent().or_else(linux_battery_percent)
}

fn macos_battery_percent() -> Option<u8> {
    command_first_line("pmset", &["-g", "batt"])
        .and_then(|first| {
            if first.contains('%') {
                Some(first)
            } else {
                Command::new("pmset")
                    .args(["-g", "batt"])
                    .output()
                    .ok()
                    .and_then(|output| {
                        output
                            .status
                            .success()
                            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
                    })
            }
        })
        .and_then(|text| parse_battery_percent(&text))
}

fn linux_battery_percent() -> Option<u8> {
    fs::read_dir("/sys/class/power_supply")
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("BAT"))
        })
        .and_then(|path| read_linux_battery_capacity(&path))
}

fn read_linux_battery_capacity(path: &Path) -> Option<u8> {
    fs::read_to_string(path.join("capacity"))
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .filter(|percent| *percent <= 100)
}

fn parse_battery_percent(text: &str) -> Option<u8> {
    let percent_index = text.find('%')?;
    let digits = text[..percent_index]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    digits.parse::<u8>().ok().filter(|percent| *percent <= 100)
}

#[cfg(test)]
mod tests {
    use super::{
        StatusLine, StatusLineContext, StatusLinePlugin, StatusSegment, parse_battery_percent,
        parse_git_status, parse_node_version, parse_rust_version,
    };
    use crate::session::PromptMode;

    #[test]
    fn statusline_renders_segments_from_plugins_in_order() {
        let statusline = StatusLine::from_plugins(vec![
            Box::new(StaticStatusPlugin::new("pwd", "/tmp/project")),
            Box::new(StaticStatusPlugin::new("git", "main clean")),
        ]);
        let context = StatusLineContext {
            mode: PromptMode::Agent,
            provider: "codex",
            cwd: std::path::Path::new("/tmp/project"),
        };

        assert_eq!(
            statusline.render(&context),
            "mode=> provider=codex pwd=/tmp/project git=main clean"
        );
    }

    #[test]
    fn git_status_parser_summarizes_clean_and_dirty_states() {
        assert_eq!(parse_git_status("## main\n"), Some("main clean".to_owned()));
        assert_eq!(
            parse_git_status("## main...origin/main\n M src/main.rs\n?? src/statusline.rs\n"),
            Some("main 2 changed".to_owned())
        );
    }

    #[test]
    fn version_parsers_return_plain_versions() {
        assert_eq!(parse_node_version("v25.9.0"), Some("25.9.0".to_owned()));
        assert_eq!(
            parse_rust_version("rustc 1.94.1 (abc 2026-01-01)"),
            Some("1.94.1".to_owned())
        );
    }

    #[test]
    fn battery_parser_reads_percent_from_pmset_output() {
        assert_eq!(
            parse_battery_percent(
                "Now drawing from 'Battery Power'\n -InternalBattery-0\t82%; discharging;"
            ),
            Some(82)
        );
    }

    struct StaticStatusPlugin {
        segment: StatusSegment,
    }

    impl StaticStatusPlugin {
        fn new(name: &str, value: &str) -> Self {
            Self {
                segment: StatusSegment::new(name, value),
            }
        }
    }

    impl StatusLinePlugin for StaticStatusPlugin {
        fn segment(&self, _context: &StatusLineContext<'_>) -> Option<StatusSegment> {
            Some(self.segment.clone())
        }
    }
}
