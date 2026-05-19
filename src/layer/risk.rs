use crate::protocol::Warning;
use regex::Regex;
use serde::Deserialize;

struct RiskPattern {
    regex: Regex,
    warning: String,
}

#[derive(Debug, Deserialize)]
struct RiskPatternConfig {
    pattern: String,
    warning: String,
}

#[derive(Debug, Deserialize)]
struct RiskPatternsFile {
    risk_patterns: Vec<RiskPatternConfig>,
}

pub struct RiskLayer {
    patterns: Vec<RiskPattern>,
}

impl RiskLayer {
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
            Ok(content) => match toml::from_str::<RiskPatternsFile>(&content) {
                Ok(file) => {
                    for p in file.risk_patterns {
                        if let Ok(regex) = Regex::new(&p.pattern) {
                            self.patterns.push(RiskPattern {
                                regex,
                                warning: p.warning,
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to parse risk patterns: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("failed to read risk patterns: {e}");
            }
        }
    }

    pub fn check(&self, input: &str) -> Option<Warning> {
        for pattern in &self.patterns {
            if pattern.regex.is_match(input) {
                return Some(Warning {
                    text: pattern.warning.clone(),
                });
            }
        }
        None
    }
}

fn builtin_patterns() -> Vec<RiskPattern> {
    let raw = vec![
        (
            r"rm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+|--force\s+).*(/|\*|~)",
            "This will permanently delete files — make sure you have the right path",
        ),
        (
            r"rm\s+-[a-zA-Z]*r[a-zA-Z]*f",
            "This will recursively force-delete — double check the target",
        ),
        (
            r"git\s+push\s+.*--force",
            "Force push will overwrite remote history — this cannot be undone",
        ),
        (
            r"git\s+push\s+-f",
            "Force push will overwrite remote history — this cannot be undone",
        ),
        (
            r"git\s+reset\s+--hard",
            "Hard reset will discard all uncommitted changes",
        ),
        (
            r"git\s+clean\s+-[a-zA-Z]*f",
            "This will delete untracked files permanently",
        ),
        (
            r"chmod\s+777",
            "Setting 777 permissions makes files world-writable — usually not what you want",
        ),
        (
            r"chmod\s+-R\s+777",
            "Recursively setting 777 is a security risk",
        ),
        (
            r"curl\s+.*\|\s*(sudo\s+)?(ba)?sh",
            "Piping curl to shell executes remote code — review the script first",
        ),
        (
            r"wget\s+.*\|\s*(sudo\s+)?(ba)?sh",
            "Piping wget to shell executes remote code — review the script first",
        ),
        (
            r"kubectl\s+delete\s+(namespace|ns)\s",
            "Deleting a namespace removes everything in it",
        ),
        (
            r"kubectl\s+delete\s+.*--all",
            "This will delete all matching resources",
        ),
        (
            r"dd\s+if=.*of=/dev/",
            "Writing directly to a device — make sure of= target is correct",
        ),
        (
            r"mkfs\.",
            "Formatting a filesystem will destroy all data on the partition",
        ),
        (
            r">\s*/dev/sd[a-z]",
            "Writing directly to a block device — this will destroy data",
        ),
        (
            r":\(\)\s*\{\s*:\|:&\s*\}\s*;:",
            "This is a fork bomb — it will crash the system",
        ),
        (
            r"sudo\s+rm\s+-[a-zA-Z]*r[a-zA-Z]*f?\s+/\s",
            "Deleting from root with sudo — extremely dangerous",
        ),
    ];

    raw.into_iter()
        .filter_map(|(pattern, warning)| {
            Regex::new(pattern).ok().map(|regex| RiskPattern {
                regex,
                warning: warning.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rm_rf() {
        let layer = RiskLayer::new();
        assert!(layer.check("rm -rf /tmp/*").is_some());
        assert!(layer.check("rm -rf ~/Documents").is_some());
    }

    #[test]
    fn test_git_force_push() {
        let layer = RiskLayer::new();
        assert!(layer.check("git push --force").is_some());
        assert!(layer.check("git push -f origin main").is_some());
        assert!(layer.check("git push origin main").is_none());
    }

    #[test]
    fn test_git_reset_hard() {
        let layer = RiskLayer::new();
        assert!(layer.check("git reset --hard HEAD~1").is_some());
        assert!(layer.check("git reset --soft HEAD~1").is_none());
    }

    #[test]
    fn test_chmod_777() {
        let layer = RiskLayer::new();
        assert!(layer.check("chmod 777 file.txt").is_some());
        assert!(layer.check("chmod -R 777 /var/www").is_some());
        assert!(layer.check("chmod 644 file.txt").is_none());
    }

    #[test]
    fn test_curl_pipe_sh() {
        let layer = RiskLayer::new();
        assert!(
            layer
                .check("curl -fsSL https://example.com/install.sh | sh")
                .is_some()
        );
        assert!(
            layer
                .check("curl -fsSL https://example.com/install.sh | sudo bash")
                .is_some()
        );
    }

    #[test]
    fn test_kubectl_delete() {
        let layer = RiskLayer::new();
        assert!(layer.check("kubectl delete namespace production").is_some());
        assert!(layer.check("kubectl delete pods --all").is_some());
        assert!(layer.check("kubectl get pods").is_none());
    }

    #[test]
    fn test_dd() {
        let layer = RiskLayer::new();
        assert!(layer.check("dd if=/dev/zero of=/dev/sda").is_some());
    }

    #[test]
    fn test_safe_commands() {
        let layer = RiskLayer::new();
        assert!(layer.check("ls -la").is_none());
        assert!(layer.check("git status").is_none());
        assert!(layer.check("docker ps").is_none());
        assert!(layer.check("npm install").is_none());
    }
}
