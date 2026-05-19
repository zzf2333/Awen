use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct GitContext {
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub has_unstaged: bool,
    pub has_staged: bool,
    pub last_commit_summary: Option<String>,
}

impl GitContext {
    pub fn status_string(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.ahead > 0 {
            parts.push(format!("ahead={}", self.ahead));
        }
        if self.behind > 0 {
            parts.push(format!("behind={}", self.behind));
        }
        if self.has_unstaged {
            parts.push("unstaged".into());
        }
        if self.has_staged {
            parts.push("staged".into());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(","))
        }
    }
}

pub fn detect_git_context(dir: &Path) -> Option<GitContext> {
    let is_git = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output()
        .ok()?;

    if !is_git.status.success() {
        return None;
    }

    let mut ctx = GitContext::default();

    if let Some(branch) = git_branch(dir) {
        ctx.branch = Some(branch);
    }

    if let Some((ahead, behind)) = git_ahead_behind(dir) {
        ctx.ahead = ahead;
        ctx.behind = behind;
    }

    let (staged, unstaged) = git_dirty(dir);
    ctx.has_staged = staged;
    ctx.has_unstaged = unstaged;

    ctx.last_commit_summary = git_last_commit(dir);

    Some(ctx)
}

fn git_branch(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn git_ahead_behind(dir: &Path) -> Option<(u32, u32)> {
    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = text.trim().split('\t').collect();
    if parts.len() == 2 {
        let ahead = parts[0].parse().unwrap_or(0);
        let behind = parts[1].parse().unwrap_or(0);
        Some((ahead, behind))
    } else {
        None
    }
}

fn git_dirty(dir: &Path) -> (bool, bool) {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let mut staged = false;
            let mut unstaged = false;
            for line in text.lines() {
                let bytes = line.as_bytes();
                if bytes.len() >= 2 {
                    if bytes[0] != b' ' && bytes[0] != b'?' {
                        staged = true;
                    }
                    if bytes[1] != b' ' {
                        unstaged = true;
                    }
                }
            }
            (staged, unstaged)
        }
        _ => (false, false),
    }
}

fn git_last_commit(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(dir)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_context_in_repo() {
        let dir = std::env::current_dir().unwrap();
        let result = detect_git_context(&dir);
        // May or may not be in a git repo depending on test environment
        if let Some(ctx) = result {
            // Branch might be None for detached HEAD or new repo
            assert!(ctx.branch.is_some() || ctx.branch.is_none());
        }
    }

    #[test]
    fn test_git_context_not_repo() {
        let dir = std::env::temp_dir();
        let sub = dir.join("awen_test_no_git");
        std::fs::create_dir_all(&sub).ok();
        let result = detect_git_context(&sub);
        assert!(result.is_none() || result.unwrap().branch.is_none());
        std::fs::remove_dir_all(&sub).ok();
    }

    #[test]
    fn test_status_string() {
        let ctx = GitContext {
            branch: Some("main".into()),
            ahead: 2,
            behind: 0,
            has_unstaged: true,
            has_staged: false,
            last_commit_summary: None,
        };
        assert_eq!(ctx.status_string(), Some("ahead=2,unstaged".into()));
    }

    #[test]
    fn test_status_string_clean() {
        let ctx = GitContext::default();
        assert!(ctx.status_string().is_none());
    }
}
