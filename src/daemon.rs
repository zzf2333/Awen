use crate::arbitrator::Arbitrator;
use crate::config::{self, AwenConfig};
use crate::context::ContextEngine;
use crate::layer::ai;
use crate::layer::failure::FailureLayer;
use crate::layer::history::HistoryLayer;
use crate::layer::risk::RiskLayer;
use crate::layer::specs::SpecsLayer;
use crate::protocol::*;

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;

struct DaemonState {
    context: ContextEngine,
    history: HistoryLayer,
    specs: SpecsLayer,
    failure: FailureLayer,
    risk: RiskLayer,
    ai_provider: Option<Arc<dyn ai::AiProvider>>,
    config: AwenConfig,
    start_time: std::time::Instant,
    last_ai_request_at: Option<std::time::Instant>,
}

fn should_request_ai(
    input: &str,
    has_warning: bool,
    max_local_confidence: f64,
    last_ai_request_at: Option<std::time::Instant>,
    debounce_ms: u64,
) -> bool {
    if input.len() < 3 {
        return false;
    }
    if has_warning {
        return false;
    }
    if max_local_confidence >= 0.9 {
        return false;
    }
    if let Some(last_at) = last_ai_request_at {
        let elapsed = last_at.elapsed().as_millis() as u64;
        if elapsed < debounce_ms {
            return false;
        }
    }
    true
}

pub async fn run(config: AwenConfig) {
    let paths = DaemonPaths {
        socket: config::socket_path(),
        db: config::history_db_path(),
        config_dir: config::config_dir(),
    };
    write_pid();
    run_on_paths(config, &paths).await;
    cleanup_pid();
}

pub struct DaemonPaths {
    pub socket: std::path::PathBuf,
    pub db: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
}

pub async fn run_on_paths(config: AwenConfig, paths: &DaemonPaths) {
    if paths.socket.exists() {
        std::fs::remove_file(&paths.socket).ok();
    }

    if let Some(parent) = paths.socket.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let history = match HistoryLayer::new(&paths.db) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("failed to open history db: {e}");
            eprintln!("failed to open history db: {e}");
            return;
        }
    };

    let mut specs = SpecsLayer::new();
    specs.load_builtin_specs();
    specs.load_user_specs(&paths.config_dir.join("specs"));

    let mut failure = FailureLayer::new();
    failure.load_user_patterns(&paths.config_dir.join("failure_patterns.toml"));

    let mut risk = RiskLayer::new();
    risk.load_user_patterns(&paths.config_dir.join("risk_patterns.toml"));

    let ai_provider = ai::create_provider(&config);

    let state = Arc::new(Mutex::new(DaemonState {
        context: ContextEngine::new(&config),
        history,
        specs,
        failure,
        risk,
        ai_provider,
        config: config.clone(),
        start_time: std::time::Instant::now(),
        last_ai_request_at: None,
    }));

    let listener = match UnixListener::bind(&paths.socket) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind socket: {e}");
            eprintln!("failed to bind socket at {}: {e}", paths.socket.display());
            return;
        }
    };

    tracing::info!("listening on {}", paths.socket.display());
    println!("awen daemon running (socket: {})", paths.socket.display());

    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_clone.notify_one();
    });

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _)) => {
                        let state = state.clone();
                        let shutdown = shutdown.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, state, shutdown).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!("accept error: {e}");
                    }
                }
            }
            _ = shutdown.notified() => {
                tracing::info!("shutting down");
                break;
            }
        }
    }

    std::fs::remove_file(&paths.socket).ok();
    println!("awen daemon stopped");
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: Arc<Mutex<DaemonState>>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let response = match serde_json::from_str::<Request>(line.trim()) {
                    Ok(request) => process_request(request, &state, &shutdown).await,
                    Err(e) => Response::Error {
                        message: format!("invalid request: {e}"),
                    },
                };

                let mut resp_json = serde_json::to_string(&response).unwrap();
                resp_json.push('\n');
                if writer.write_all(resp_json.as_bytes()).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

async fn process_request(
    request: Request,
    state: &Arc<Mutex<DaemonState>>,
    shutdown: &Arc<tokio::sync::Notify>,
) -> Response {
    match request {
        Request::Suggest(req) => handle_suggest(req, state).await,
        Request::Record(req) => handle_record(req, state).await,
        Request::Status => handle_status(state).await,
        Request::Context => handle_context(state).await,
        Request::Shutdown => {
            shutdown.notify_one();
            Response::Ok
        }
    }
}

async fn handle_suggest(req: SuggestRequest, state: &Arc<Mutex<DaemonState>>) -> Response {
    let (
        mut response,
        ai_provider,
        max_tokens,
        stderr_max_chars,
        has_warning,
        debounce_ms,
        last_ai_request_at,
    ) = {
        let mut state = state.lock().await;
        state.context.update_cwd(req.context.cwd.clone());

        let mut suggestions = Vec::new();

        let history_results = state.history.suggest(&req.input, &req.context.cwd, 5);
        suggestions.extend(history_results);

        let specs_results = state.specs.suggest(&req.input, req.cursor_pos);
        suggestions.extend(specs_results);

        let mut hint = None;
        if let Some(exit_code) = req.context.last_exit_code
            && exit_code != 0
            && let Some(stderr) = &req.context.last_stderr
            && let Some((fail_suggestion, fail_hint)) =
                state.failure.match_failure(stderr, exit_code)
        {
            suggestions.push(fail_suggestion);
            hint = Some(fail_hint);
        }

        let warning = if state.config.ui.risk_detection {
            state.risk.check(&req.input)
        } else {
            None
        };

        let has_warning = warning.is_some();
        let response = Arbitrator::arbitrate(suggestions, &req.context, hint, warning);
        let ai_provider = state.ai_provider.clone();
        let max_tokens = state.config.ai.max_tokens;
        let stderr_max_chars = state.config.context.stderr_max_chars;
        let debounce_ms = state.config.ai.debounce_ms;
        let last_ai_request_at = state.last_ai_request_at;

        (
            response,
            ai_provider,
            max_tokens,
            stderr_max_chars,
            has_warning,
            debounce_ms,
            last_ai_request_at,
        )
    };

    if let Some(provider) = ai_provider {
        let max_local_confidence = response
            .suggestions
            .iter()
            .map(|s| s.confidence)
            .fold(0.0_f64, f64::max);

        if should_request_ai(
            &req.input,
            has_warning,
            max_local_confidence,
            last_ai_request_at,
            debounce_ms,
        ) {
            let mut ctx = req.context.clone();
            crate::sanitize::sanitize_request_context(&mut ctx, stderr_max_chars);

            let prompt = ai::build_prompt(&req.input, &ctx);

            match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                provider.complete(&prompt, max_tokens),
            )
            .await
            {
                Ok(Ok(ai_response)) => {
                    if let Some(suggestion) = ai::parse_ai_suggestion(&req.input, &ai_response) {
                        Arbitrator::merge_ai_suggestion(&mut response, suggestion);
                    }
                }
                Ok(Err(e)) => {
                    tracing::debug!("AI completion error: {e}");
                }
                Err(_) => {
                    tracing::debug!("AI completion timed out");
                }
            }

            let mut state = state.lock().await;
            state.last_ai_request_at = Some(std::time::Instant::now());
        }
    }

    Response::Suggest(response)
}

async fn handle_record(req: RecordCommandRequest, state: &Arc<Mutex<DaemonState>>) -> Response {
    let mut state = state.lock().await;

    state.context.record_command(
        req.command.clone(),
        req.exit_code,
        req.stderr.clone(),
        req.cwd.clone(),
        req.duration_ms,
    );

    if let Err(e) = state.history.record(&req.command, &req.cwd, req.exit_code) {
        tracing::warn!("failed to record command: {e}");
    }

    Response::Ok
}

async fn handle_status(state: &Arc<Mutex<DaemonState>>) -> Response {
    let state = state.lock().await;
    Response::Status(StatusResponse {
        running: true,
        pid: std::process::id(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        history_count: state.history.count(),
        ai_enabled: state.config.ai.enabled,
    })
}

async fn handle_context(state: &Arc<Mutex<DaemonState>>) -> Response {
    let mut state = state.lock().await;
    let ctx = state.context.build_context_response();
    Response::Context(ctx)
}

pub fn is_running() -> bool {
    let socket_path = config::socket_path();
    if !socket_path.exists() {
        return false;
    }
    std::os::unix::net::UnixStream::connect(&socket_path).is_ok()
}

pub fn read_pid() -> Option<u32> {
    let pid_path = config::pid_path();
    std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn write_pid() {
    let pid_path = config::pid_path();
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(pid_path, std::process::id().to_string()).ok();
}

fn cleanup_pid() {
    std::fs::remove_file(config::pid_path()).ok();
}

pub fn cleanup_socket() {
    std::fs::remove_file(config::socket_path()).ok();
    cleanup_pid();
}

pub async fn send_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let response = send_request(&Request::Shutdown).await?;
    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => Err(message.into()),
        _ => Err("unexpected response".into()),
    }
}

pub async fn send_status_request() -> Result<Response, Box<dyn std::error::Error>> {
    send_request(&Request::Status).await
}

pub async fn send_context_request() -> Result<Response, Box<dyn std::error::Error>> {
    send_request(&Request::Context).await
}

async fn send_request(request: &Request) -> Result<Response, Box<dyn std::error::Error>> {
    send_request_to(&config::socket_path(), request).await
}

pub async fn send_request_to(
    socket_path: &std::path::Path,
    request: &Request,
) -> Result<Response, Box<dyn std::error::Error>> {
    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    let mut json = serde_json::to_string(request)?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: Response = serde_json::from_str(line.trim())?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_request_ai_short_input() {
        assert!(!should_request_ai("gi", false, 0.0, None, 300));
        assert!(!should_request_ai("ab", false, 0.0, None, 300));
    }

    #[test]
    fn test_should_request_ai_warning_present() {
        assert!(!should_request_ai("rm -rf /", true, 0.0, None, 300));
    }

    #[test]
    fn test_should_request_ai_high_confidence() {
        assert!(!should_request_ai("git checkout", false, 0.95, None, 300));
        assert!(!should_request_ai("git checkout", false, 0.9, None, 300));
    }

    #[test]
    fn test_should_request_ai_debounce() {
        let recent = std::time::Instant::now();
        assert!(!should_request_ai(
            "docker run",
            false,
            0.0,
            Some(recent),
            300,
        ));
        assert!(!should_request_ai(
            "cargo build",
            false,
            0.0,
            Some(recent),
            300,
        ));
    }

    #[test]
    fn test_should_request_ai_happy_path() {
        assert!(should_request_ai("docker run", false, 0.5, None, 300));
        assert!(should_request_ai("git checkout", false, 0.89, None, 300));
    }
}
