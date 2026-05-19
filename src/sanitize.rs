use regex::Regex;
use std::sync::LazyLock;

static SENSITIVE_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(api[_-]?key|secret|token|password|passwd|credential|auth|private[_-]?key|access[_-]?key)").unwrap()
});

pub static SENSITIVE_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xoxb-[a-zA-Z0-9\-]+|AKIA[0-9A-Z]{16}|Bearer\s+[A-Za-z0-9\-._~+/]{20,}=*|\w+://[^:]+:[^@\s]+@)",
    )
    .unwrap()
});

pub static SENSITIVE_COMMAND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(docker\s+login\s+.*-p\s+\S+|export\s+\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL)\w*=\S+|mysql\s+.*-p\S+)",
    )
    .unwrap()
});

pub fn sanitize_env_hints(hints: &[String]) -> Vec<String> {
    hints
        .iter()
        .filter(|h| {
            if let Some((key, _)) = h.split_once('=') {
                !SENSITIVE_KEY_RE.is_match(key)
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

pub fn sanitize_stderr(stderr: &str, max_chars: usize) -> String {
    let truncated = if stderr.len() > max_chars {
        &stderr[..max_chars]
    } else {
        stderr
    };
    SENSITIVE_VALUE_RE
        .replace_all(truncated, "[REDACTED]")
        .to_string()
}

pub fn sanitize_command(command: &str) -> String {
    let result = SENSITIVE_VALUE_RE.replace_all(command, "[REDACTED]");
    SENSITIVE_COMMAND_RE
        .replace_all(&result, "[REDACTED]")
        .to_string()
}

pub fn sanitize_cwd(cwd: &str) -> String {
    if is_sensitive_path(cwd) {
        "[SENSITIVE_PATH]".to_string()
    } else {
        cwd.to_string()
    }
}

pub fn sanitize_request_context(
    ctx: &mut crate::protocol::RequestContext,
    stderr_max_chars: usize,
) {
    ctx.cwd = sanitize_cwd(&ctx.cwd);
    if let Some(ref cmd) = ctx.last_command {
        ctx.last_command = Some(sanitize_command(cmd));
    }
    if let Some(ref stderr) = ctx.last_stderr {
        ctx.last_stderr = Some(sanitize_stderr(stderr, stderr_max_chars));
    }
    ctx.session_commands = ctx
        .session_commands
        .iter()
        .map(|c| sanitize_command(c))
        .collect();
    if let Some(ref branch) = ctx.git_branch {
        ctx.git_branch = Some(
            SENSITIVE_VALUE_RE
                .replace_all(branch, "[REDACTED]")
                .to_string(),
        );
    }
    if let Some(ref status) = ctx.git_status {
        ctx.git_status = Some(
            SENSITIVE_VALUE_RE
                .replace_all(status, "[REDACTED]")
                .to_string(),
        );
    }
    ctx.env_hints = sanitize_env_hints(&ctx.env_hints);
}

pub fn is_sensitive_path(path: &str) -> bool {
    let sensitive_patterns = [
        ".env",
        ".ssh",
        "id_rsa",
        "id_ed25519",
        "kubeconfig",
        ".aws/credentials",
        ".gnupg",
        "wallet",
        "private_key",
        "keystore",
        ".netrc",
        ".npmrc",
        ".pypirc",
    ];
    let lower = path.to_lowercase();
    sensitive_patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_env_hints() {
        let hints = vec![
            "NODE_ENV=development".into(),
            "API_KEY=sk-1234567890abcdefghijklmnop".into(),
            "HOME=/home/user".into(),
            "SECRET_TOKEN=abc123".into(),
            "LANG=en_US.UTF-8".into(),
        ];
        let result = sanitize_env_hints(&hints);
        assert_eq!(result.len(), 3);
        assert!(result.contains(&"NODE_ENV=development".into()));
        assert!(result.contains(&"HOME=/home/user".into()));
        assert!(result.contains(&"LANG=en_US.UTF-8".into()));
    }

    #[test]
    fn test_sanitize_stderr_truncation() {
        let long_stderr = "a".repeat(1000);
        let result = sanitize_stderr(&long_stderr, 500);
        assert_eq!(result.len(), 500);
    }

    #[test]
    fn test_sanitize_stderr_redacts_tokens() {
        let stderr = "Error: invalid token sk-abcdefghijklmnopqrstuvwxyz12345";
        let result = sanitize_stderr(stderr, 500);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz12345"));
    }

    #[test]
    fn test_sanitize_command_docker_login() {
        let result = sanitize_command("docker login registry.io -u user -p mysecret123");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("mysecret123"));
    }

    #[test]
    fn test_sanitize_command_export_secret() {
        let result = sanitize_command("export API_KEY=sk-abcdefghijklmnopqrstuvwxyz12345");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz12345"));
    }

    #[test]
    fn test_sanitize_command_database_url() {
        let result = sanitize_command("DATABASE_URL=postgres://user:pass123@localhost:5432/db");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("pass123"));
    }

    #[test]
    fn test_sanitize_command_bearer_token() {
        let result = sanitize_command(
            "curl -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc.def'",
        );
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_command_aws_key() {
        let result = sanitize_command("aws configure set aws_access_key_id AKIAIOSFODNN7EXAMPLE");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_sanitize_command_safe() {
        let result = sanitize_command("cargo build --release");
        assert_eq!(result, "cargo build --release");

        let result = sanitize_command("git push origin main");
        assert_eq!(result, "git push origin main");
    }

    #[test]
    fn test_sanitize_cwd_sensitive() {
        assert_eq!(sanitize_cwd("/home/user/.ssh"), "[SENSITIVE_PATH]");
        assert_eq!(sanitize_cwd("/home/user/.env"), "[SENSITIVE_PATH]");
        assert_eq!(
            sanitize_cwd("/home/user/.aws/credentials"),
            "[SENSITIVE_PATH]"
        );
    }

    #[test]
    fn test_sanitize_cwd_normal() {
        assert_eq!(sanitize_cwd("/home/user/project"), "/home/user/project");
        assert_eq!(sanitize_cwd("/tmp"), "/tmp");
    }

    #[test]
    fn test_sanitize_request_context_all_fields() {
        use crate::protocol::RequestContext;
        let mut ctx = RequestContext {
            cwd: "/home/user/.ssh".into(),
            last_command: Some(
                "curl -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc.def'"
                    .into(),
            ),
            last_exit_code: Some(1),
            last_stderr: Some(
                "error with token sk-abcdefghijklmnopqrstuvwxyz12345 involved".into(),
            ),
            git_branch: Some("main".into()),
            git_status: Some("ahead=1".into()),
            session_commands: vec![
                "docker login registry.io -u user -p secret".into(),
                "ls -la".into(),
            ],
            env_hints: vec!["NODE_ENV=dev".into(), "API_KEY=secret123".into()],
        };
        sanitize_request_context(&mut ctx, 500);
        assert_eq!(ctx.cwd, "[SENSITIVE_PATH]");
        assert!(ctx.last_command.as_ref().unwrap().contains("[REDACTED]"));
        assert!(ctx.last_stderr.as_ref().unwrap().contains("[REDACTED]"));
        assert!(ctx.session_commands[0].contains("[REDACTED]"));
        assert_eq!(ctx.session_commands[1], "ls -la");
        assert_eq!(ctx.env_hints.len(), 1);
        assert_eq!(ctx.env_hints[0], "NODE_ENV=dev");
    }

    #[test]
    fn test_sanitize_request_context_git_injection() {
        use crate::protocol::RequestContext;
        let mut ctx = RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: None,
            last_stderr: None,
            git_branch: Some("feat/sk-abcdefghijklmnopqrstuvwxyz12345".into()),
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        };
        sanitize_request_context(&mut ctx, 500);
        assert!(ctx.git_branch.as_ref().unwrap().contains("[REDACTED]"));
        assert!(
            !ctx.git_branch
                .as_ref()
                .unwrap()
                .contains("sk-abcdefghijklmnopqrstuvwxyz12345")
        );
    }

    #[test]
    fn test_is_sensitive_path() {
        assert!(is_sensitive_path("/home/user/.ssh/id_rsa"));
        assert!(is_sensitive_path("/home/user/.env"));
        assert!(is_sensitive_path("/home/user/.aws/credentials"));
        assert!(is_sensitive_path("/home/user/.gnupg/private_key"));
        assert!(!is_sensitive_path("/home/user/project/src/main.rs"));
        assert!(!is_sensitive_path("/home/user/documents/readme.md"));
    }
}
