use std::path::PathBuf;
use std::time::Duration;

use awen::config::AwenConfig;
use awen::daemon::{self, DaemonPaths};
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

impl TestDaemon {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = test_paths(dir.path());
        let socket_path = paths.socket.clone();
        let config = test_config();

        tokio::spawn(async move {
            daemon::run_on_paths(config, &paths).await;
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
