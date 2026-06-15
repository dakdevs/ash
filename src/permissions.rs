use std::collections::BTreeMap;

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
    if pattern == "*" {
        return true;
    }

    let pattern = pattern.as_bytes();
    let input = input.as_bytes();
    let mut pattern_index = 0;
    let mut input_index = 0;
    let mut star_index = None;
    let mut star_match_index = 0;

    while input_index < input.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == input[input_index] || pattern[pattern_index] == b'?')
        {
            pattern_index += 1;
            input_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            star_match_index = input_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_match_index += 1;
            input_index = star_match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
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
