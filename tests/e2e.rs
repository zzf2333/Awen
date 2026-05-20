use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use awen::config::AwenConfig;
use awen::daemon::{self, DaemonPaths};
use awen::layer::ai::{AiError, AiProvider};
use awen::protocol::*;

fn test_paths(dir: &std::path::Path) -> DaemonPaths {
    DaemonPaths {
        socket: dir.join("awen-test.sock"),
        db: dir.join("history-test.db"),
        config_dir: dir.join("config"),
    }
}

fn test_config() -> AwenConfig {
    let mut config = AwenConfig::default();
    config.ai.enabled = false;
    config.context.repo_detect = false;
    config.context.git_context = false;
    config
}

struct TestDaemon {
    socket_path: PathBuf,
    _dir: tempfile::TempDir,
}

struct SlowMockProvider;

#[async_trait::async_trait]
impl AiProvider for SlowMockProvider {
    async fn complete(&self, _prompt: &str, _max_tokens: u32) -> Result<String, AiError> {
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok("mock-slow-result".into())
    }

    async fn complete_nl(&self, _prompt: &str, _max_tokens: u32) -> Result<String, AiError> {
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok("echo hello".into())
    }
}

struct FastMockProvider {
    call_count: std::sync::atomic::AtomicU32,
}

impl FastMockProvider {
    fn new() -> Self {
        Self {
            call_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AiProvider for FastMockProvider {
    async fn complete(&self, _prompt: &str, _max_tokens: u32) -> Result<String, AiError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok("pods --all-namespaces".into())
    }

    async fn complete_nl(&self, _prompt: &str, _max_tokens: u32) -> Result<String, AiError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok("kubectl get pods --all-namespaces".into())
    }
}

impl TestDaemon {
    async fn start() -> Self {
        Self::start_with_config_and_ai(test_config(), None).await
    }

    async fn start_with_ai(config: AwenConfig, provider: Arc<dyn AiProvider>) -> Self {
        Self::start_with_config_and_ai(config, Some(provider)).await
    }

    async fn start_with_config_and_ai(
        config: AwenConfig,
        ai_override: Option<Arc<dyn AiProvider>>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = test_paths(dir.path());
        let socket_path = paths.socket.clone();

        tokio::spawn(async move {
            daemon::run_on_paths_with_ai(config, &paths, ai_override).await;
        });

        for _ in 0..50 {
            if socket_path.exists() {
                if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        assert!(
            socket_path.exists(),
            "daemon socket not created within timeout"
        );

        Self {
            socket_path,
            _dir: dir,
        }
    }

    async fn send(&self, request: &Request) -> Response {
        daemon::send_request_to(&self.socket_path, request)
            .await
            .expect("failed to send request")
    }

    async fn shutdown(self) {
        let resp = self.send(&Request::Shutdown).await;
        assert!(matches!(resp, Response::Ok));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ============================================================
// Daemon 生命周期测试
// ============================================================

#[tokio::test]
async fn test_daemon_start_and_status() {
    let daemon = TestDaemon::start().await;

    let resp = daemon.send(&Request::Status).await;
    match resp {
        Response::Status(s) => {
            assert!(s.running);
            assert!(!s.ai_enabled);
            assert_eq!(s.history_count, 0);
        }
        other => panic!("expected Status response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_daemon_shutdown() {
    let daemon = TestDaemon::start().await;
    let socket_path = daemon.socket_path.clone();

    daemon.shutdown().await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !socket_path.exists(),
        "socket file should be cleaned up after shutdown"
    );
}

#[tokio::test]
async fn test_daemon_multiple_connections() {
    let daemon = TestDaemon::start().await;

    let (r1, r2, r3) = tokio::join!(
        daemon.send(&Request::Status),
        daemon.send(&Request::Status),
        daemon.send(&Request::Status),
    );

    assert!(matches!(r1, Response::Status(_)));
    assert!(matches!(r2, Response::Status(_)));
    assert!(matches!(r3, Response::Status(_)));

    daemon.shutdown().await;
}

// ============================================================
// Record + Context 追踪测试
// ============================================================

#[tokio::test]
async fn test_record_and_context() {
    let daemon = TestDaemon::start().await;

    let record_req = Request::Record(RecordCommandRequest {
        command: "cargo build".into(),
        exit_code: 0,
        stderr: None,
        cwd: "/home/user/project".into(),
        duration_ms: Some(1500),
    });
    let resp = daemon.send(&record_req).await;
    assert!(matches!(resp, Response::Ok));

    let record_req2 = Request::Record(RecordCommandRequest {
        command: "cargo test".into(),
        exit_code: 1,
        stderr: Some("test result: FAILED. 1 passed; 2 failed".into()),
        cwd: "/home/user/project".into(),
        duration_ms: Some(3200),
    });
    daemon.send(&record_req2).await;

    let resp = daemon.send(&Request::Context).await;
    match resp {
        Response::Context(c) => {
            assert_eq!(c.cwd, "/home/user/project");
            assert!(c.recent_commands.contains(&"cargo build".to_string()));
            assert!(c.recent_commands.contains(&"cargo test".to_string()));
            assert_eq!(c.last_exit_code, Some(1));
        }
        other => panic!("expected Context response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_context_cwd_tracking() {
    let daemon = TestDaemon::start().await;

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "ls".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/tmp/dir1".into(),
            duration_ms: None,
        }))
        .await;

    let resp = daemon.send(&Request::Context).await;
    match &resp {
        Response::Context(c) => assert_eq!(c.cwd, "/tmp/dir1"),
        other => panic!("expected Context, got: {other:?}"),
    }

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "pwd".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/tmp/dir2".into(),
            duration_ms: None,
        }))
        .await;

    let resp = daemon.send(&Request::Context).await;
    match resp {
        Response::Context(c) => assert_eq!(c.cwd, "/tmp/dir2"),
        other => panic!("expected Context, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// Suggest 管道 E2E 测试
// ============================================================

#[tokio::test]
async fn test_suggest_specs_completion() {
    let daemon = TestDaemon::start().await;

    let req = Request::Suggest(SuggestRequest {
        input: "git ch".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                !s.suggestions.is_empty(),
                "should get specs suggestions for 'git ch'"
            );
            let has_checkout = s.suggestions.iter().any(|sg| sg.text.contains("checkout"));
            let has_cherry_pick = s
                .suggestions
                .iter()
                .any(|sg| sg.text.contains("cherry-pick"));
            assert!(
                has_checkout || has_cherry_pick,
                "should suggest git checkout or cherry-pick, got: {:?}",
                s.suggestions.iter().map(|sg| &sg.text).collect::<Vec<_>>()
            );
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source == SuggestionSource::Specs)
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_suggest_history_after_record() {
    let daemon = TestDaemon::start().await;

    for _ in 0..3 {
        daemon
            .send(&Request::Record(RecordCommandRequest {
                command: "docker compose up -d".into(),
                exit_code: 0,
                stderr: None,
                cwd: "/home/user/project".into(),
                duration_ms: Some(500),
            }))
            .await;
    }

    let req = Request::Suggest(SuggestRequest {
        input: "docker".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let has_history = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::History);
            assert!(
                has_history,
                "should get history suggestions after recording commands, got: {:?}",
                s.suggestions
            );
            let has_compose = s.suggestions.iter().any(|sg| sg.text.contains("compose"));
            assert!(
                has_compose,
                "history should contain 'docker compose up -d', got: {:?}",
                s.suggestions.iter().map(|sg| &sg.text).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_suggest_failure_recovery() {
    let daemon = TestDaemon::start().await;

    let req = Request::Suggest(SuggestRequest {
        input: "".into(),
        cursor_pos: 0,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: Some("cargo build".into()),
            last_exit_code: Some(1),
            last_stderr: Some("error[E0432]: unresolved import `tokio`\ncould not find `tokio` in the list of imported crates".into()),
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let has_failure = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Failure);
            assert!(
                has_failure,
                "should get failure recovery suggestion for missing crate, got: {:?}",
                s.suggestions
            );
            let has_cargo_add = s
                .suggestions
                .iter()
                .any(|sg| sg.text.contains("cargo add tokio"));
            assert!(
                has_cargo_add,
                "failure should suggest 'cargo add tokio', got: {:?}",
                s.suggestions.iter().map(|sg| &sg.text).collect::<Vec<_>>()
            );
            assert!(s.hint.is_some(), "should have a failure recovery hint");
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_suggest_risk_warning() {
    let daemon = TestDaemon::start().await;

    let req = Request::Suggest(SuggestRequest {
        input: "rm -rf /".into(),
        cursor_pos: 8,
        context: RequestContext {
            cwd: "/home/user".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                s.warning.is_some(),
                "should get risk warning for 'rm -rf /'"
            );
            let warning_text = s.warning.unwrap().text.to_lowercase();
            assert!(
                warning_text.contains("delete")
                    || warning_text.contains("rm")
                    || warning_text.contains("危"),
                "warning should mention danger, got: {warning_text}"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_suggest_no_warning_for_safe_command() {
    let daemon = TestDaemon::start().await;

    let req = Request::Suggest(SuggestRequest {
        input: "ls -la".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/home/user".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                s.warning.is_none(),
                "should NOT get warning for safe command 'ls -la'"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// 完整流程测试：Record → Suggest → Context
// ============================================================

#[tokio::test]
async fn test_full_session_flow() {
    let daemon = TestDaemon::start().await;

    // 1. 初始状态
    let resp = daemon.send(&Request::Status).await;
    match &resp {
        Response::Status(s) => {
            assert!(s.running);
            assert_eq!(s.history_count, 0);
        }
        other => panic!("expected Status, got: {other:?}"),
    }

    // 2. 模拟用户执行命令
    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "npm install".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/home/user/webapp".into(),
            duration_ms: Some(12000),
        }))
        .await;

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "npm run dev".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/home/user/webapp".into(),
            duration_ms: Some(500),
        }))
        .await;

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "npm test".into(),
            exit_code: 1,
            stderr: Some(r#"npm ERR! Missing script: "test""#.into()),
            cwd: "/home/user/webapp".into(),
            duration_ms: Some(200),
        }))
        .await;

    // 3. history count 增长
    let resp = daemon.send(&Request::Status).await;
    match &resp {
        Response::Status(s) => assert!(
            s.history_count >= 3,
            "should have at least 3 history entries"
        ),
        other => panic!("expected Status, got: {other:?}"),
    }

    // 4. Context 反映最后的失败
    let resp = daemon.send(&Request::Context).await;
    match &resp {
        Response::Context(c) => {
            assert_eq!(c.cwd, "/home/user/webapp");
            assert_eq!(c.last_exit_code, Some(1));
        }
        other => panic!("expected Context, got: {other:?}"),
    }

    // 5. Suggest：失败后应给出 failure 建议
    let req = Request::Suggest(SuggestRequest {
        input: "".into(),
        cursor_pos: 0,
        context: RequestContext {
            cwd: "/home/user/webapp".into(),
            last_command: Some("npm test".into()),
            last_exit_code: Some(1),
            last_stderr: Some(r#"npm ERR! Missing script: "test""#.into()),
            git_branch: None,
            git_status: None,
            session_commands: vec![
                "npm install".into(),
                "npm run dev".into(),
                "npm test".into(),
            ],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });
    let resp = daemon.send(&req).await;
    match &resp {
        Response::Suggest(s) => {
            let has_failure = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Failure);
            assert!(
                has_failure,
                "should suggest failure recovery after npm test failure"
            );
            assert!(s.hint.is_some(), "should have failure hint");
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }

    // 6. Suggest：输入 npm 应有 history 和 specs 建议
    let req = Request::Suggest(SuggestRequest {
        input: "npm ".into(),
        cursor_pos: 4,
        context: RequestContext {
            cwd: "/home/user/webapp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });
    let resp = daemon.send(&req).await;
    match &resp {
        Response::Suggest(s) => {
            assert!(
                !s.suggestions.is_empty(),
                "should have suggestions for 'npm '"
            );
            let sources: Vec<_> = s.suggestions.iter().map(|sg| sg.source).collect();
            let has_specs = sources.contains(&SuggestionSource::Specs);
            let has_history = sources.contains(&SuggestionSource::History);
            assert!(
                has_specs || has_history,
                "should have specs or history suggestions for 'npm ', got sources: {sources:?}"
            );
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }

    // 7. 风险检测仍然在工作
    let req = Request::Suggest(SuggestRequest {
        input: "git push --force".into(),
        cursor_pos: 16,
        context: RequestContext {
            cwd: "/home/user/webapp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: Some("main".into()),
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });
    let resp = daemon.send(&req).await;
    match &resp {
        Response::Suggest(s) => {
            assert!(s.warning.is_some(), "should warn about 'git push --force'");
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// 协议错误处理测试
// ============================================================

#[tokio::test]
async fn test_invalid_json_request() {
    let daemon = TestDaemon::start().await;

    let stream = tokio::net::UnixStream::connect(&daemon.socket_path)
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    writer.write_all(b"{\"bad json\n").await.unwrap();

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    let resp: Response = serde_json::from_str(line.trim()).unwrap();
    match resp {
        Response::Error { message } => {
            assert!(
                message.contains("invalid"),
                "error should mention invalid request, got: {message}"
            );
        }
        other => panic!("expected Error response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// Security tests
// ============================================================

#[tokio::test]
async fn test_record_sensitive_command_skipped() {
    let daemon = TestDaemon::start().await;

    let initial_status = daemon.send(&Request::Status).await;
    let initial_count = match initial_status {
        Response::Status(s) => s.history_count,
        _ => panic!("expected Status"),
    };

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "docker login registry.io -u user -p secret123".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/tmp".into(),
            duration_ms: None,
        }))
        .await;

    let status = daemon.send(&Request::Status).await;
    match status {
        Response::Status(s) => {
            assert_eq!(
                s.history_count, initial_count,
                "sensitive command should not be recorded"
            );
        }
        other => panic!("expected Status, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_suggest_with_sensitive_context() {
    let daemon = TestDaemon::start().await;

    let response = daemon
        .send(&Request::Suggest(SuggestRequest {
            input: "git status".into(),
            cursor_pos: 10,
            context: RequestContext {
                cwd: "/tmp".into(),
                last_command: Some(
                    "curl -H 'Authorization: Bearer sk-abcdefghijklmnopqrstuvwxyz12345'".into(),
                ),
                last_exit_code: Some(1),
                last_stderr: Some("error: token sk-abcdefghijklmnopqrstuvwxyz12345 invalid".into()),
                git_branch: Some("main".into()),
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
            skip_ai: false,
            nl_mode: false,
        }))
        .await;

    match response {
        Response::Suggest(_) => {}
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_risk_warning_with_suggestions_coexist() {
    let daemon = TestDaemon::start().await;

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "rm -rf ./build".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/project".into(),
            duration_ms: None,
        }))
        .await;

    let response = daemon
        .send(&Request::Suggest(SuggestRequest {
            input: "rm -rf /".into(),
            cursor_pos: 8,
            context: RequestContext {
                cwd: "/project".into(),
                last_command: None,
                last_exit_code: None,
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
            skip_ai: false,
            nl_mode: false,
        }))
        .await;

    match response {
        Response::Suggest(s) => {
            assert!(s.warning.is_some(), "risk warning should be present");
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// AI fallback tests
// ============================================================

#[tokio::test]
async fn test_ai_disabled_local_suggestions_work() {
    let daemon = TestDaemon::start().await;

    daemon
        .send(&Request::Record(RecordCommandRequest {
            command: "cargo build --release".into(),
            exit_code: 0,
            stderr: None,
            cwd: "/home/user/project".into(),
            duration_ms: Some(5000),
        }))
        .await;

    let req = Request::Suggest(SuggestRequest {
        input: "cargo".into(),
        cursor_pos: 5,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                !s.suggestions.is_empty(),
                "should get local suggestions even with AI disabled"
            );
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "no suggestion should come from AI when disabled"
            );
            let has_local = s.suggestions.iter().any(|sg| {
                sg.source == SuggestionSource::History || sg.source == SuggestionSource::Specs
            });
            assert!(has_local, "should have history or specs suggestions");
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_ai_timeout_returns_local_suggestions() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.ai.timeout_ms = 500;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider: Arc<dyn AiProvider> = Arc::new(SlowMockProvider);
    let daemon = TestDaemon::start_with_ai(config, provider).await;

    let req = Request::Suggest(SuggestRequest {
        input: "git ch".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let start = std::time::Instant::now();
    let resp = daemon.send(&req).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "response should arrive within 1s despite slow AI (took {:?})",
        elapsed
    );

    match resp {
        Response::Suggest(s) => {
            assert!(
                !s.suggestions.is_empty(),
                "should still get local suggestions when AI times out"
            );
            let has_specs = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Specs);
            assert!(
                has_specs,
                "should have specs suggestions for 'git ch', got: {:?}",
                s.suggestions
                    .iter()
                    .map(|sg| &sg.source)
                    .collect::<Vec<_>>()
            );
            let has_ai = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Ai);
            assert!(!has_ai, "AI suggestion should not be present (timed out)");
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// AI merge + debounce E2E tests
// ============================================================

#[tokio::test]
async fn test_ai_suggestion_merged_into_response() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider: Arc<dyn AiProvider> = Arc::new(FastMockProvider::new());
    let daemon = TestDaemon::start_with_ai(config, provider).await;

    let req = Request::Suggest(SuggestRequest {
        input: "myapp deploy".into(),
        cursor_pos: 12,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let ai_suggestions: Vec<_> = s
                .suggestions
                .iter()
                .filter(|sg| sg.source == SuggestionSource::Ai)
                .collect();
            assert!(
                !ai_suggestions.is_empty(),
                "should have AI suggestion merged into response, got sources: {:?}",
                s.suggestions.iter().map(|sg| sg.source).collect::<Vec<_>>()
            );
            assert!(
                ai_suggestions[0].text.contains("pods"),
                "AI suggestion should contain mock completion, got: {}",
                ai_suggestions[0].text
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_ai_debounce_skips_second_request() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 5000;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let make_req = |input: &str| {
        Request::Suggest(SuggestRequest {
            input: input.into(),
            cursor_pos: input.len(),
            context: RequestContext {
                cwd: "/home/user/project".into(),
                last_command: None,
                last_exit_code: Some(0),
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
            skip_ai: false,
            nl_mode: false,
        })
    };

    let resp1 = daemon.send(&make_req("myapp deploy")).await;
    match &resp1 {
        Response::Suggest(s) => {
            assert!(
                s.suggestions
                    .iter()
                    .any(|sg| sg.source == SuggestionSource::Ai),
                "first request should trigger AI"
            );
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }
    assert_eq!(
        provider.calls(),
        1,
        "AI should be called once for first request"
    );

    let resp2 = daemon.send(&make_req("myapp rollback")).await;
    match &resp2 {
        Response::Suggest(s) => {
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "second request within debounce window should NOT have AI suggestion"
            );
        }
        other => panic!("expected Suggest, got: {other:?}"),
    }
    assert_eq!(
        provider.calls(),
        1,
        "AI should NOT be called again within debounce window"
    );

    daemon.shutdown().await;
}

// ============================================================
// skip_ai E2E tests
// ============================================================

#[tokio::test]
async fn test_skip_ai_returns_local_only() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "myapp deploy".into(),
        cursor_pos: 12,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: true,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "skip_ai=true should not include AI suggestions, got: {:?}",
                s.suggestions.iter().map(|sg| sg.source).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }
    assert_eq!(
        provider.calls(),
        0,
        "AI provider should never be called when skip_ai=true"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_skip_ai_fast_response_with_slow_provider() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider: Arc<dyn AiProvider> = Arc::new(SlowMockProvider);
    let daemon = TestDaemon::start_with_ai(config, provider).await;

    let req = Request::Suggest(SuggestRequest {
        input: "git ch".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: true,
        nl_mode: false,
    });

    let start = std::time::Instant::now();
    let resp = daemon.send(&req).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "skip_ai=true should return in <100ms even with slow AI provider (took {:?})",
        elapsed
    );

    match resp {
        Response::Suggest(s) => {
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "skip_ai=true should not have AI suggestions"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_skip_ai_false_still_calls_ai() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "myapp deploy".into(),
        cursor_pos: 12,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let has_ai = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Ai);
            assert!(
                has_ai,
                "skip_ai=false should include AI suggestions, got: {:?}",
                s.suggestions.iter().map(|sg| sg.source).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }
    assert_eq!(
        provider.calls(),
        1,
        "AI provider should be called when skip_ai=false"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_skip_ai_false_risk_warning_still_skips_ai() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "rm -rf /".into(),
        cursor_pos: 8,
        context: RequestContext {
            cwd: "/home/user".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(s.warning.is_some(), "should still detect risk warning");
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "AI should be skipped when risk warning is present"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }
    assert_eq!(
        provider.calls(),
        0,
        "AI should not be called when risk warning is present"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_skip_ai_false_high_confidence_local_skips_ai() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    for _ in 0..10 {
        daemon
            .send(&Request::Record(RecordCommandRequest {
                command: "git checkout main".into(),
                exit_code: 0,
                stderr: None,
                cwd: "/home/user/project".into(),
                duration_ms: None,
            }))
            .await;
    }

    let req = Request::Suggest(SuggestRequest {
        input: "git checkout main".into(),
        cursor_pos: 17,
        context: RequestContext {
            cwd: "/home/user/project".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let max_confidence = s
                .suggestions
                .iter()
                .map(|sg| sg.confidence)
                .fold(0.0_f64, f64::max);
            if max_confidence >= 0.9 {
                assert!(
                    s.suggestions
                        .iter()
                        .all(|sg| sg.source != SuggestionSource::Ai),
                    "AI should be skipped when local confidence >= 0.9"
                );
                assert_eq!(
                    provider.calls(),
                    0,
                    "AI should not be called when local confidence is high"
                );
            }
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// AI-as-fallback E2E tests
// ============================================================

#[tokio::test]
async fn test_ai_not_called_when_local_sufficient() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    for _ in 0..5 {
        daemon
            .send(&Request::Record(RecordCommandRequest {
                command: "git commit -m 'fix'".into(),
                exit_code: 0,
                stderr: None,
                cwd: "/tmp".into(),
                duration_ms: None,
            }))
            .await;
    }

    let req = Request::Suggest(SuggestRequest {
        input: "git co".into(),
        cursor_pos: 6,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                !s.suggestions.is_empty(),
                "should have local suggestions for 'git co'"
            );
            assert!(
                s.suggestions
                    .iter()
                    .all(|sg| sg.source != SuggestionSource::Ai),
                "AI should not be called when local has sufficient results"
            );
            assert_eq!(
                provider.calls(),
                0,
                "AI provider should not be called when local candidates are sufficient"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_ai_called_when_local_insufficient() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "myapp deploy --region".into(),
        cursor_pos: 21,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                provider.calls() > 0,
                "AI should be called when local candidates are insufficient"
            );
            let has_ai = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Ai);
            assert!(has_ai, "response should contain AI suggestion");
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_ai_error_recovery_when_failure_unmatched() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "".into(),
        cursor_pos: 0,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: Some("myapp migrate".into()),
            last_exit_code: Some(1),
            last_stderr: Some("database connection timeout after 30s".into()),
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                provider.calls() > 0,
                "AI should be called for error recovery when local patterns miss"
            );
            let has_ai = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Ai);
            assert!(
                has_ai,
                "should have AI recovery suggestion for unmatched error"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_ai_not_called_when_local_failure_pattern_matches() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    let req = Request::Suggest(SuggestRequest {
        input: "".into(),
        cursor_pos: 0,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: Some("ripgrep".into()),
            last_exit_code: Some(127),
            last_stderr: Some("zsh: command not found: ripgrep".into()),
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: false,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            let has_failure = s
                .suggestions
                .iter()
                .any(|sg| sg.source == SuggestionSource::Failure);
            assert!(has_failure, "should have local failure recovery suggestion");
            assert_eq!(
                provider.calls(),
                0,
                "AI should not be called when local failure pattern matches"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_need_ai_signal_in_phase1_response() {
    let mut config = AwenConfig::default();
    config.ai.enabled = true;
    config.ai.debounce_ms = 0;
    config.context.repo_detect = false;
    config.context.git_context = false;

    let provider = Arc::new(FastMockProvider::new());
    let provider_clone: Arc<dyn AiProvider> = provider.clone();
    let daemon = TestDaemon::start_with_ai(config, provider_clone).await;

    // Unknown command: local insufficient → need_ai should be true
    let req = Request::Suggest(SuggestRequest {
        input: "myapp deploy".into(),
        cursor_pos: 12,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: true,
        nl_mode: false,
    });

    let resp = daemon.send(&req).await;
    match resp {
        Response::Suggest(s) => {
            assert!(
                s.need_ai,
                "need_ai should be true when local candidates are insufficient"
            );
            assert_eq!(provider.calls(), 0, "skip_ai=true should not call AI");
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    // Known command with specs: local sufficient → need_ai should be false
    let req2 = Request::Suggest(SuggestRequest {
        input: "git ".into(),
        cursor_pos: 4,
        context: RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        },
        timestamp: None,
        skip_ai: true,
        nl_mode: false,
    });

    let resp2 = daemon.send(&req2).await;
    match resp2 {
        Response::Suggest(s) => {
            assert!(
                !s.need_ai,
                "need_ai should be false when local candidates are sufficient"
            );
        }
        other => panic!("expected Suggest response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

// ============================================================
// NL Generate protocol tests
// ============================================================

#[tokio::test]
async fn test_nl_generate_no_ai_returns_none() {
    let daemon = TestDaemon::start().await;

    let resp = daemon
        .send(&Request::NlGenerate(NlGenerateRequest {
            query: "list files in current directory".into(),
            context: RequestContext {
                cwd: "/tmp".into(),
                last_command: None,
                last_exit_code: None,
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
        }))
        .await;

    match resp {
        Response::NlGenerate(r) => {
            assert!(
                r.command.is_none(),
                "no AI provider, command should be None"
            );
            assert!(r.warning.is_none());
        }
        other => panic!("expected NlGenerate response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_nl_generate_empty_query_returns_none() {
    let daemon = TestDaemon::start().await;

    let resp = daemon
        .send(&Request::NlGenerate(NlGenerateRequest {
            query: "".into(),
            context: RequestContext {
                cwd: "/tmp".into(),
                last_command: None,
                last_exit_code: None,
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
        }))
        .await;

    match resp {
        Response::NlGenerate(r) => {
            assert!(r.command.is_none(), "empty query should return None");
        }
        other => panic!("expected NlGenerate response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_nl_generate_with_ai_provider() {
    let mut config = test_config();
    config.ai.enabled = true;
    config.ai.timeout_ms = 5000;

    let provider = Arc::new(FastMockProvider::new());
    let daemon = TestDaemon::start_with_ai(config, provider.clone()).await;

    let resp = daemon
        .send(&Request::NlGenerate(NlGenerateRequest {
            query: "get all pods across namespaces".into(),
            context: RequestContext {
                cwd: "/tmp".into(),
                last_command: None,
                last_exit_code: None,
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
        }))
        .await;

    match resp {
        Response::NlGenerate(r) => {
            assert!(r.command.is_some(), "AI provider should produce a command");
            assert!(provider.calls() > 0, "AI provider should have been called");
        }
        other => panic!("expected NlGenerate response, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn test_legacy_suggest_with_nl_mode_compat() {
    let daemon = TestDaemon::start().await;

    let resp = daemon
        .send(&Request::Suggest(SuggestRequest {
            input: "# list files".into(),
            cursor_pos: 12,
            context: RequestContext {
                cwd: "/tmp".into(),
                last_command: None,
                last_exit_code: None,
                last_stderr: None,
                git_branch: None,
                git_status: None,
                session_commands: vec![],
                env_hints: vec![],
            },
            timestamp: None,
            skip_ai: false,
            nl_mode: true,
        }))
        .await;

    match resp {
        Response::NlGenerate(r) => {
            assert!(
                r.command.is_none(),
                "no AI provider, compat path should return NlGenerate with None"
            );
        }
        other => panic!("expected NlGenerate response from legacy compat path, got: {other:?}"),
    }

    daemon.shutdown().await;
}
