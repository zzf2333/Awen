use crate::protocol::{Suggestion, SuggestionSource};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct SpecFile {
    pub command: CommandSpec,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub subcommands: Vec<SubcommandSpec>,
    #[serde(default)]
    pub flags: Vec<FlagSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubcommandSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub flags: Vec<FlagSpec>,
    #[serde(default)]
    pub subcommands: Vec<SubcommandSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlagSpec {
    pub name: String,
    #[serde(default)]
    pub short: Option<String>,
    #[serde(default)]
    pub arg: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub struct SpecsLayer {
    specs: HashMap<String, CommandSpec>,
}

impl Default for SpecsLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl SpecsLayer {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
        }
    }

    pub fn load_builtin_specs(&mut self) {
        macro_rules! builtin_specs {
            ($($name:literal),* $(,)?) => {
                [$(($name, include_str!(concat!("../../specs/", $name, ".toml")))),*]
            };
        }

        let builtin = builtin_specs![
            // VCS & dev ecosystem
            "git",
            "docker",
            "npm",
            "cargo",
            "brew",
            "curl",
            "ssh",
            // Cloud & infrastructure
            "gh",
            "kubectl",
            "terraform",
            // Languages & runtimes
            "python",
            "go",
            "node",
            // Package managers & build tools
            "pip",
            "cmake",
            "make",
            // AI tools
            "claude",
            "codex",
            "opencode",
            "antigravity",
            // Linux core - file operations
            "ls",
            "rm",
            "cp",
            "mv",
            "mkdir",
            "touch",
            "ln",
            "chmod",
            "chown",
            // Linux text processing & utilities
            "cat",
            "head",
            "tail",
            "grep",
            "sed",
            "awk",
            "sort",
            "uniq",
            "wc",
            "diff",
            "cut",
            "tr",
            "tee",
            "xargs",
            // Linux core - search, archive, process, disk
            "find",
            "tar",
            "ps",
            "kill",
            "df",
            "du",
            "lsof",
            // Networking & diagnostics
            "ping",
            "dig",
            "wget",
            "ss",
            "nmap",
            // System administration
            "systemctl",
            "journalctl",
            "htop",
            // Terminal multiplexers
            "tmux",
            "screen",
            // Database CLIs
            "psql",
            "mysql",
            "redis-cli",
            "mongosh",
            "sqlite3",
        ];

        for (name, content) in builtin {
            match toml::from_str::<SpecFile>(content) {
                Ok(spec) => {
                    self.specs.insert(name.to_string(), spec.command);
                }
                Err(e) => {
                    tracing::warn!("failed to parse builtin spec {name}: {e}");
                }
            }
        }
    }

    pub fn load_user_specs(&mut self, dir: &Path) {
        if !dir.exists() {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str::<SpecFile>(&content) {
                        Ok(spec) => {
                            self.specs.insert(spec.command.name.clone(), spec.command);
                        }
                        Err(e) => {
                            tracing::warn!("failed to parse spec {}: {e}", path.display());
                        }
                    },
                    Err(e) => {
                        tracing::warn!("failed to read spec {}: {e}", path.display());
                    }
                }
            }
        }
    }

    pub fn suggest(&self, input: &str, _cursor_pos: usize) -> Vec<Suggestion> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Vec::new();
        }

        let cmd_name = parts[0];
        let spec = match self.specs.get(cmd_name) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let remaining = &parts[1..];
        let partial = if input.ends_with(' ') {
            ""
        } else {
            remaining.last().copied().unwrap_or("")
        };
        let completed_parts = if input.ends_with(' ') {
            remaining
        } else if remaining.is_empty() {
            &[]
        } else {
            &remaining[..remaining.len() - 1]
        };

        let (target_flags, target_subcmds) = resolve_subcommand(spec, completed_parts);

        let mut suggestions = Vec::new();

        if !partial.starts_with('-') {
            for sub in target_subcmds {
                if sub.name.starts_with(partial) {
                    suggestions.push(Suggestion {
                        text: sub.name.clone(),
                        source: SuggestionSource::Specs,
                        confidence: 1.0,
                        description: sub.description.clone(),
                    });
                }
            }
        }

        for flag in target_flags {
            if flag.name.starts_with(partial)
                || partial.is_empty()
                || partial == "-"
                || partial == "--"
            {
                suggestions.push(Suggestion {
                    text: flag.name.clone(),
                    source: SuggestionSource::Specs,
                    confidence: 1.0,
                    description: flag.description.clone().map(|d| {
                        if let Some(arg) = &flag.arg {
                            format!("{d} ({arg})")
                        } else {
                            d
                        }
                    }),
                });
            }

            if let Some(short) = &flag.short
                && short.starts_with(partial)
                && !partial.is_empty()
                && partial != "-"
            {
                suggestions.push(Suggestion {
                    text: short.clone(),
                    source: SuggestionSource::Specs,
                    confidence: 1.0,
                    description: flag.description.clone(),
                });
            }
        }

        suggestions
    }

    pub fn has_spec(&self, command: &str) -> bool {
        self.specs.contains_key(command)
    }

    pub fn spec_count(&self) -> usize {
        self.specs.len()
    }
}

fn resolve_subcommand<'a>(
    spec: &'a CommandSpec,
    parts: &[&str],
) -> (&'a [FlagSpec], &'a [SubcommandSpec]) {
    let mut current_flags = spec.flags.as_slice();
    let mut current_subcmds = spec.subcommands.as_slice();

    for &part in parts {
        if part.starts_with('-') {
            continue;
        }
        if let Some(sub) = current_subcmds.iter().find(|s| s.name == part) {
            current_flags = sub.flags.as_slice();
            current_subcmds = sub.subcommands.as_slice();
        }
    }

    (current_flags, current_subcmds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spec() {
        let toml_str = r#"
[command]
name = "test"
description = "test command"

[[command.subcommands]]
name = "sub1"
description = "subcommand 1"

[[command.subcommands.flags]]
name = "--verbose"
short = "-v"
description = "verbose output"
"#;
        let spec: SpecFile = toml::from_str(toml_str).unwrap();
        assert_eq!(spec.command.name, "test");
        assert_eq!(spec.command.subcommands.len(), 1);
        assert_eq!(spec.command.subcommands[0].flags.len(), 1);
    }

    #[test]
    fn test_suggest_subcommands() {
        let mut layer = SpecsLayer::new();
        layer.specs.insert(
            "git".into(),
            CommandSpec {
                name: "git".into(),
                description: None,
                subcommands: vec![
                    SubcommandSpec {
                        name: "commit".into(),
                        description: Some("Record changes".into()),
                        flags: vec![],
                        subcommands: vec![],
                    },
                    SubcommandSpec {
                        name: "checkout".into(),
                        description: Some("Switch branches".into()),
                        flags: vec![],
                        subcommands: vec![],
                    },
                ],
                flags: vec![],
            },
        );

        let results = layer.suggest("git c", 6);
        assert!(!results.is_empty());
        assert!(results.iter().any(|s| s.text == "commit"));
        assert!(results.iter().any(|s| s.text == "checkout"));
    }

    #[test]
    fn test_suggest_flags() {
        let mut layer = SpecsLayer::new();
        layer.specs.insert(
            "git".into(),
            CommandSpec {
                name: "git".into(),
                description: None,
                subcommands: vec![SubcommandSpec {
                    name: "commit".into(),
                    description: None,
                    flags: vec![
                        FlagSpec {
                            name: "--message".into(),
                            short: Some("-m".into()),
                            arg: Some("MSG".into()),
                            description: Some("Commit message".into()),
                        },
                        FlagSpec {
                            name: "--amend".into(),
                            short: None,
                            arg: None,
                            description: Some("Amend previous commit".into()),
                        },
                    ],
                    subcommands: vec![],
                }],
                flags: vec![],
            },
        );

        let results = layer.suggest("git commit --", 13);
        assert!(results.iter().any(|s| s.text == "--message"));
        assert!(results.iter().any(|s| s.text == "--amend"));
    }

    #[test]
    fn test_suggest_unknown_command() {
        let layer = SpecsLayer::new();
        let results = layer.suggest("unknown_cmd --flag", 18);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggest_empty() {
        let layer = SpecsLayer::new();
        let results = layer.suggest("", 0);
        assert!(results.is_empty());
    }
}
