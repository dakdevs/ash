use std::collections::BTreeMap;

use globset::Glob;

use crate::error::{AshError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    Ask,
    Deny,
}

impl PermissionAction {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "allow" => Ok(Self::Allow),
            "ask" => Ok(Self::Ask),
            "deny" => Ok(Self::Deny),
            other => Err(AshError::UnknownPermissionAction(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    pattern: String,
    action: PermissionAction,
}

impl PermissionRule {
    pub fn new(pattern: impl Into<String>, action: PermissionAction) -> Self {
        Self {
            pattern: pattern.into(),
            action,
        }
    }

    fn matches(&self, input: &str) -> bool {
        wildcard_matches(&self.pattern, input)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSet {
    rules: BTreeMap<String, Vec<PermissionRule>>,
    fallback: PermissionAction,
}

impl PermissionSet {
    #[must_use]
    pub fn secure_default() -> Self {
        let mut set = Self {
            rules: BTreeMap::new(),
            fallback: PermissionAction::Ask,
        };
        set.set_rule("read", PermissionRule::new("*", PermissionAction::Allow));
        set.set_rule("read", PermissionRule::new("*.env", PermissionAction::Deny));
        set.set_rule(
            "read",
            PermissionRule::new("*.env.*", PermissionAction::Deny),
        );
        set.set_rule(
            "read",
            PermissionRule::new("*.env.example", PermissionAction::Allow),
        );
        set.set_rule("bash", PermissionRule::new("*", PermissionAction::Ask));
        set
    }

    pub fn set_rule(&mut self, tool: impl Into<String>, rule: PermissionRule) {
        self.rules.entry(tool.into()).or_default().push(rule);
    }

    #[must_use]
    pub fn resolve(&self, tool: &str, input: &str) -> PermissionAction {
        self.resolve_rules(tool, input)
            .or_else(|| self.resolve_rules("*", input))
            .unwrap_or(self.fallback)
    }

    fn resolve_rules(&self, tool: &str, input: &str) -> Option<PermissionAction> {
        self.rules.get(tool).and_then(|rules| {
            rules
                .iter()
                .rev()
                .find(|rule| rule.matches(input))
                .map(|rule| rule.action)
        })
    }
}

fn wildcard_matches(pattern: &str, input: &str) -> bool {
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher().is_match(input))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{PermissionAction, PermissionSet};

    #[test]
    fn secure_default_denies_env_reads() {
        let permissions = PermissionSet::secure_default();

        assert_eq!(
            permissions.resolve("read", "src/lib.rs"),
            PermissionAction::Allow
        );
        assert_eq!(permissions.resolve("read", ".env"), PermissionAction::Deny);
        assert_eq!(
            permissions.resolve("read", ".env.local"),
            PermissionAction::Deny
        );
        assert_eq!(
            permissions.resolve("read", ".env.example"),
            PermissionAction::Allow
        );
    }

    #[test]
    fn later_rules_override_earlier_rules() {
        let mut permissions = PermissionSet::secure_default();

        permissions.set_rule(
            "bash",
            super::PermissionRule::new("git status*", PermissionAction::Allow),
        );

        assert_eq!(
            permissions.resolve("bash", "git status --short"),
            PermissionAction::Allow
        );
        assert_eq!(
            permissions.resolve("bash", "rm -rf ."),
            PermissionAction::Ask
        );
    }
}
