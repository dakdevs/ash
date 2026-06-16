use std::{collections::BTreeMap, sync::OnceLock, time::Duration};

use serde::Deserialize;

const CLI_SPINNERS_JSON: &str = include_str!("../vendor/cli-spinners/spinners.json");
const DEFAULT_SPINNER: &str = "dots";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CliSpinner {
    interval: u64,
    frames: Vec<String>,
}

impl CliSpinner {
    #[must_use]
    pub const fn interval_ms(&self) -> u64 {
        self.interval
    }

    #[must_use]
    pub const fn interval(&self) -> Duration {
        Duration::from_millis(self.interval)
    }

    #[must_use]
    pub fn frames(&self) -> &[String] {
        &self.frames
    }
}

#[must_use]
pub fn spinner(name: &str) -> Option<&'static CliSpinner> {
    catalog().get(name)
}

#[must_use]
pub fn default_spinner() -> Option<&'static CliSpinner> {
    spinner(DEFAULT_SPINNER)
}

#[must_use]
pub fn spinner_frame(name: &str, frame_index: usize) -> Option<&'static str> {
    let spinner = spinner(name)?;
    if spinner.frames.is_empty() {
        return None;
    }

    Some(spinner.frames[frame_index % spinner.frames.len()].as_str())
}

fn catalog() -> &'static BTreeMap<String, CliSpinner> {
    static CATALOG: OnceLock<BTreeMap<String, CliSpinner>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(CLI_SPINNERS_JSON).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{default_spinner, spinner, spinner_frame};

    #[test]
    fn loads_cli_spinners_catalog() {
        let dots = spinner("dots").expect("dots spinner");

        assert_eq!(dots.interval_ms(), 80);
        assert_eq!(
            dots.frames(),
            ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        );
    }

    #[test]
    fn default_spinner_is_dots() {
        assert_eq!(default_spinner(), spinner("dots"));
    }

    #[test]
    fn spinner_frame_wraps_by_frame_count() {
        assert_eq!(spinner_frame("dots", 0), Some("⠋"));
        assert_eq!(spinner_frame("dots", 10), Some("⠋"));
    }
}
