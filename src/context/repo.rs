use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoType {
    Rust,
    Node,
    Python,
    Go,
    DockerCompose,
    Docker,
    Nix,
    Make,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub repo_type: RepoType,
    pub variant: Option<String>,
}

impl std::fmt::Display for RepoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoType::Rust => write!(f, "rust"),
            RepoType::Node => write!(f, "node"),
            RepoType::Python => write!(f, "python"),
            RepoType::Go => write!(f, "go"),
            RepoType::DockerCompose => write!(f, "docker-compose"),
            RepoType::Docker => write!(f, "docker"),
            RepoType::Nix => write!(f, "nix"),
            RepoType::Make => write!(f, "make"),
        }
    }
}

pub fn detect_repo_type(dir: &Path) -> Option<RepoInfo> {
    if dir.join("Cargo.toml").exists() {
        let variant = detect_rust_variant(dir);
        return Some(RepoInfo {
            repo_type: RepoType::Rust,
            variant,
        });
    }

    if dir.join("package.json").exists() {
        let variant = detect_node_variant(dir);
        return Some(RepoInfo {
            repo_type: RepoType::Node,
            variant,
        });
    }

    if dir.join("pyproject.toml").exists() || dir.join("requirements.txt").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::Python,
            variant: None,
        });
    }

    if dir.join("go.mod").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::Go,
            variant: None,
        });
    }

    if dir.join("docker-compose.yml").exists() || dir.join("docker-compose.yaml").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::DockerCompose,
            variant: None,
        });
    }

    if dir.join("Dockerfile").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::Docker,
            variant: None,
        });
    }

    if dir.join("flake.nix").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::Nix,
            variant: None,
        });
    }

    if dir.join("Makefile").exists() {
        return Some(RepoInfo {
            repo_type: RepoType::Make,
            variant: None,
        });
    }

    None
}

fn detect_rust_variant(dir: &Path) -> Option<String> {
    if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml"))
        && content.contains("[workspace]")
    {
        return Some("workspace".into());
    }
    None
}

fn detect_node_variant(dir: &Path) -> Option<String> {
    if dir.join("pnpm-workspace.yaml").exists() {
        return Some("pnpm".into());
    }
    if dir.join("turbo.json").exists() {
        return Some("turbo".into());
    }
    if dir.join("nx.json").exists() {
        return Some("nx".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_rust() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Rust);
        assert!(info.variant.is_none());
    }

    #[test]
    fn test_detect_rust_workspace() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]",
        )
        .unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Rust);
        assert_eq!(info.variant.as_deref(), Some("workspace"));
    }

    #[test]
    fn test_detect_node() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Node);
    }

    #[test]
    fn test_detect_node_pnpm() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("pnpm-workspace.yaml"), "packages:\n  - a").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Node);
        assert_eq!(info.variant.as_deref(), Some("pnpm"));
    }

    #[test]
    fn test_detect_python() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Python);
    }

    #[test]
    fn test_detect_go() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example.com/app").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Go);
    }

    #[test]
    fn test_detect_none() {
        let dir = TempDir::new().unwrap();
        assert!(detect_repo_type(dir.path()).is_none());
    }

    #[test]
    fn test_priority_order() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.path().join("Makefile"), "all:").unwrap();
        let info = detect_repo_type(dir.path()).unwrap();
        assert_eq!(info.repo_type, RepoType::Rust);
    }
}
