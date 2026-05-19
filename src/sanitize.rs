use regex::Regex;
use std::sync::LazyLock;

static SENSITIVE_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(api[_-]?key|secret|token|password|passwd|credential|auth|private[_-]?key|access[_-]?key)").unwrap()
});

static SENSITIVE_VALUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xoxb-[a-zA-Z0-9\-]+)",
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

pub fn sanitize_request_context(ctx: &mut crate::protocol::RequestContext, stderr_max_chars: usize) {
    if let Some(ref stderr) = ctx.last_stderr {
        ctx.last_stderr = Some(sanitize_stderr(stderr, stderr_max_chars));
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
    fn test_sanitize_request_context() {
        use crate::protocol::RequestContext;
        let mut ctx = RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(1),
            last_stderr: Some(
                "error with token sk-abcdefghijklmnopqrstuvwxyz12345 involved".into(),
            ),
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec!["NODE_ENV=dev".into(), "API_KEY=secret123".into()],
        };
        sanitize_request_context(&mut ctx, 500);
        assert!(ctx.last_stderr.as_ref().unwrap().contains("[REDACTED]"));
        assert!(!ctx
            .last_stderr
            .as_ref()
            .unwrap()
            .contains("sk-abcdefghijklmnopqrstuvwxyz12345"));
        assert_eq!(ctx.env_hints.len(), 1);
        assert_eq!(ctx.env_hints[0], "NODE_ENV=dev");
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
