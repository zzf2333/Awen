use crate::protocol::{Hint, HintKind, Suggestion, SuggestionSource};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Clone)]
struct FailurePattern {
    regex: Regex,
    suggestion_template: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct FailurePatternConfig {
    pattern: String,
    suggestion: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct FailurePatternsFile {
    failure_patterns: Vec<FailurePatternConfig>,
}

pub struct FailureLayer {
    patterns: Vec<FailurePattern>,
}

impl Default for FailureLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl FailureLayer {
    pub fn new() -> Self {
        Self {
            patterns: builtin_patterns(),
        }
    }

    pub fn load_user_patterns(&mut self, path: &std::path::Path) {
        if !path.exists() {
            return;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<FailurePatternsFile>(&content) {
                Ok(file) => {
                    for p in file.failure_patterns {
                        if let Ok(regex) = Regex::new(&p.pattern) {
                            self.patterns.push(FailurePattern {
                                regex,
                                suggestion_template: p.suggestion,
                                description: p.description,
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to parse failure patterns: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("failed to read failure patterns: {e}");
            }
        }
    }

    pub fn match_failure(&self, stderr: &str, exit_code: i32) -> Option<(Suggestion, Hint)> {
        if exit_code == 0 || stderr.is_empty() {
            return None;
        }

        for pattern in &self.patterns {
            if let Some(caps) = pattern.regex.captures(stderr) {
                let mut suggestion_text = pattern.suggestion_template.clone();
                let mut description_text = pattern.description.clone();
                for i in 1..caps.len() {
                    if let Some(m) = caps.get(i) {
                        let placeholder = format!("{{{i}}}");
                        suggestion_text = suggestion_text.replace(&placeholder, m.as_str());
                        description_text = description_text.replace(&placeholder, m.as_str());
                    }
                }

                return Some((
                    Suggestion {
                        text: suggestion_text,
                        source: SuggestionSource::Failure,
                        confidence: 0.95,
                        description: Some(description_text.clone()),
                    },
                    Hint {
                        text: description_text,
                        kind: HintKind::FailureRecovery,
                    },
                ));
            }
        }

        None
    }
}

fn builtin_patterns() -> Vec<FailurePattern> {
    let raw = vec![
        (
            r"cannot find crate `(\w+)`",
            "cargo add {1}",
            "Looks like `{1}` is missing",
        ),
        (
            r"Module not found.*'(\S+)'",
            "npm install {1}",
            "Looks like `{1}` is missing",
        ),
        (
            r"No module named '(\w+)'",
            "pip install {1}",
            "Looks like `{1}` is missing",
        ),
        (
            r"command not found: (\w+)",
            "brew install {1}",
            "Command `{1}` is not installed",
        ),
        (
            r"Permission denied",
            "sudo !!",
            "Permission denied — try with sudo",
        ),
        (
            r"port (\d+) already in use",
            "lsof -i :{1}",
            "Port {1} is already in use",
        ),
        (
            r"address already in use.*:(\d+)",
            "lsof -i :{1}",
            "Port {1} is already in use",
        ),
        (
            r"EADDRINUSE.*:(\d+)",
            "lsof -i :{1}",
            "Port {1} is already in use",
        ),
        (
            r"Could not resolve host: (\S+)",
            "ping {1}",
            "Cannot resolve host `{1}`",
        ),
        (
            r"fatal: not a git repository",
            "git init",
            "Not a git repository",
        ),
        (
            r"error: failed to push some refs",
            "git pull --rebase",
            "Remote has changes — pull first",
        ),
        (
            r"CONFLICT \(content\)",
            "git status",
            "Merge conflicts detected",
        ),
        (
            r"error\[E0432\]: unresolved import `(\S+)`",
            "cargo add {1}",
            "Unresolved import `{1}`",
        ),
        (
            r#"npm ERR! Missing script: "(\w+)""#,
            "npm run",
            "Script `{1}` not found — check available scripts",
        ),
        (
            r"error: package `(\S+)` cannot be found",
            "cargo search {1}",
            "Package `{1}` not found",
        ),
        (
            r"could not find `Cargo\.toml` in `(\S+)`",
            "find . -name Cargo.toml -maxdepth 3",
            "No Cargo.toml in `{1}` — wrong directory?",
        ),
        (
            r"(\w+): illegal option -- (\w)",
            "{1} --help",
            "Invalid flag `-{2}` for `{1}`",
        ),
        (
            r"(\w[\w-]*): unrecognized option '(-{1,2}\S+)'",
            "{1} --help",
            "Unknown option `{2}` for `{1}`",
        ),
    ];

    raw.into_iter()
        .filter_map(|(pattern, suggestion, description)| {
            Regex::new(pattern).ok().map(|regex| FailurePattern {
                regex,
                suggestion_template: suggestion.into(),
                description: description.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_rust_crate() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("error: cannot find crate `tokio`", 1);
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "cargo add tokio");
        assert_eq!(suggestion.source, SuggestionSource::Failure);
        assert_eq!(
            suggestion.description.as_deref(),
            Some("Looks like `tokio` is missing")
        );
        assert_eq!(hint.text, "Looks like `tokio` is missing");
    }

    #[test]
    fn test_missing_node_module() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("Error: Module not found: Can't resolve 'axios'", 1);
        assert!(result.is_some());
        let (suggestion, _) = result.unwrap();
        assert_eq!(suggestion.text, "npm install axios");
    }

    #[test]
    fn test_command_not_found() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("zsh: command not found: ripgrep", 127);
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "brew install ripgrep");
        assert_eq!(
            suggestion.description.as_deref(),
            Some("Command `ripgrep` is not installed")
        );
        assert_eq!(hint.text, "Command `ripgrep` is not installed");
    }

    #[test]
    fn test_permission_denied() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("bash: /usr/local/bin/thing: Permission denied", 1);
        assert!(result.is_some());
        let (suggestion, _) = result.unwrap();
        assert_eq!(suggestion.text, "sudo !!");
    }

    #[test]
    fn test_port_in_use() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("Error: port 3000 already in use", 1);
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "lsof -i :3000");
        assert_eq!(
            suggestion.description.as_deref(),
            Some("Port 3000 is already in use")
        );
        assert_eq!(hint.text, "Port 3000 is already in use");
    }

    #[test]
    fn test_no_match() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("some random error output", 1);
        assert!(result.is_none());
    }

    #[test]
    fn test_exit_code_zero() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("cannot find crate `tokio`", 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_cargo_toml() {
        let layer = FailureLayer::new();
        let result = layer.match_failure(
            "error: could not find `Cargo.toml` in `/Users/saonian` or any parent directory",
            101,
        );
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "find . -name Cargo.toml -maxdepth 3");
        assert_eq!(
            hint.text,
            "No Cargo.toml in `/Users/saonian` — wrong directory?"
        );
    }

    #[test]
    fn test_illegal_option() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("find: illegal option -- n", 1);
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "find --help");
        assert_eq!(hint.text, "Invalid flag `-n` for `find`");
    }

    #[test]
    fn test_unrecognized_option() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("grep: unrecognized option '--colour'", 2);
        assert!(result.is_some());
        let (suggestion, hint) = result.unwrap();
        assert_eq!(suggestion.text, "grep --help");
        assert_eq!(hint.text, "Unknown option `--colour` for `grep`");
    }

    #[test]
    fn test_git_push_rejected() {
        let layer = FailureLayer::new();
        let result = layer.match_failure("error: failed to push some refs to 'origin'", 1);
        assert!(result.is_some());
        let (suggestion, _) = result.unwrap();
        assert_eq!(suggestion.text, "git pull --rebase");
    }
}
