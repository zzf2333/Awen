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

#[allow(clippy::too_many_arguments)]
fn ai_is_useful(
    input: &str,
    has_warning: bool,
    local_candidate_count: usize,
    max_local_confidence: f64,
    has_failure_context: bool,
    local_failure_matched: bool,
    min_local_candidates: usize,
    min_local_confidence: f64,
) -> bool {
    if has_warning {
        return false;
    }
    if has_failure_context && !local_failure_matched {
        return true;
    }
    if input.len() < 2 {
        return false;
    }
    local_candidate_count < min_local_candidates && max_local_confidence < min_local_confidence
}

#[allow(clippy::too_many_arguments)]
fn should_request_ai(
    input: &str,
    has_warning: bool,
    local_candidate_count: usize,
    max_local_confidence: f64,
    has_failure_context: bool,
    local_failure_matched: bool,
    last_ai_request_at: Option<std::time::Instant>,
    debounce_ms: u64,
    min_local_candidates: usize,
    min_local_confidence: f64,
) -> bool {
    if !ai_is_useful(
        input,
        has_warning,
        local_candidate_count,
        max_local_confidence,
        has_failure_context,
        local_failure_matched,
        min_local_candidates,
        min_local_confidence,
    ) {
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
    run_on_paths_with_ai(config, paths, None).await;
}

pub async fn run_on_paths_with_ai(
    config: AwenConfig,
    paths: &DaemonPaths,
    ai_override: Option<Arc<dyn ai::AiProvider>>,
) {
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

    let need_history_import = history.count() == 0;
    let import_db_path = paths.db.clone();

    let mut specs = SpecsLayer::new();
    specs.load_builtin_specs();
    specs.load_user_specs(&paths.config_dir.join("specs"));

    let mut failure = FailureLayer::new();
    failure.load_user_patterns(&paths.config_dir.join("failure_patterns.toml"));

    let mut risk = RiskLayer::new();
    risk.load_user_patterns(&paths.config_dir.join("risk_patterns.toml"));

    let ai_provider = ai_override.or_else(|| ai::create_provider(&config));

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

    if need_history_import {
        tokio::task::spawn_blocking(move || {
            let histfile = config::default_zsh_histfile();
            if !histfile.exists() {
                tracing::info!(
                    "no zsh history at {}, skipping auto-import",
                    histfile.display()
                );
                return;
            }
            tracing::info!("empty history DB, importing from {}", histfile.display());
            match crate::layer::history_import::import_zsh_history(&import_db_path, &histfile) {
                Ok(r) => {
                    tracing::info!(
                        "history import: {} imported, {} sensitive skipped, {} empty skipped",
                        r.imported,
                        r.skipped_sensitive,
                        r.skipped_empty,
                    );
                }
                Err(e) => {
                    tracing::warn!("history import failed: {e}");
                }
            }
        });
    }

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
        Request::NlGenerate(req) => handle_nl_generate(req, state).await,
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
    // Backward compatibility: old plugin sends nl_mode=true via Suggest
    if req.nl_mode {
        let nl_req = NlGenerateRequest {
            query: req
                .input
                .strip_prefix('#')
                .unwrap_or(&req.input)
                .trim()
                .to_string(),
            context: req.context,
            timestamp: req.timestamp,
        };
        return handle_nl_generate(nl_req, state).await;
    }

    if req.input.is_empty() {
        let (
            suggestions,
            hint,
            local_failure_matched,
            has_failure_context,
            ai_provider,
            max_tokens,
            timeout_ms,
            stderr_max_chars,
            debounce_ms,
            last_ai_request_at,
        ) = {
            let state = state.lock().await;
            let mut suggestions = state.history.suggest_next(&req.context.cwd, 5);

            let mut hint = None;
            let local_failure_matched = if let Some(exit_code) = req.context.last_exit_code
                && exit_code != 0
                && let Some(stderr) = &req.context.last_stderr
                && let Some((fail_suggestion, fail_hint)) =
                    state.failure.match_failure(stderr, exit_code)
            {
                suggestions.insert(0, fail_suggestion);
                hint = Some(fail_hint);
                true
            } else {
                false
            };

            let has_failure_context = req.context.last_exit_code.is_some_and(|c| c != 0)
                && req.context.last_stderr.is_some();

            (
                suggestions,
                hint,
                local_failure_matched,
                has_failure_context,
                state.ai_provider.clone(),
                state.config.ai.max_tokens,
                state.config.ai.timeout_ms,
                state.config.context.stderr_max_chars,
                state.config.ai.debounce_ms,
                state.last_ai_request_at,
            )
        };

        let need_ai = has_failure_context && !local_failure_matched && ai_provider.is_some();

        let mut response = SuggestResponse {
            suggestions,
            hint,
            warning: None,
            need_ai,
        };

        if !req.skip_ai
            && need_ai
            && let Some(provider) = ai_provider
        {
            let debounce_ok = last_ai_request_at
                .map(|t| t.elapsed().as_millis() as u64 >= debounce_ms)
                .unwrap_or(true);
            if debounce_ok {
                {
                    let mut st = state.lock().await;
                    st.last_ai_request_at = Some(std::time::Instant::now());
                }
                let mut ctx = req.context.clone();
                crate::sanitize::sanitize_request_context(&mut ctx, stderr_max_chars);
                let prompt = ai::build_error_recovery_prompt(&ctx);
                tracing::info!("AI error recovery started");

                let ai_start = std::time::Instant::now();
                match tokio::time::timeout(
                    std::time::Duration::from_millis(timeout_ms),
                    provider.complete_nl(&prompt, max_tokens),
                )
                .await
                {
                    Ok(Ok(ai_response)) => {
                        tracing::info!(
                            "AI error recovery received, latency_ms={}",
                            ai_start.elapsed().as_millis()
                        );
                        if let Some(suggestion) = ai::parse_nl_suggestion(&ai_response) {
                            Arbitrator::merge_ai_suggestion(&mut response, suggestion);
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::info!("AI error recovery error: {e}");
                    }
                    Err(_) => {
                        tracing::info!(
                            "AI error recovery timed out, latency_ms={}",
                            ai_start.elapsed().as_millis()
                        );
                    }
                }
            }
        }

        return Response::Suggest(response);
    }

    let (
        mut response,
        local_failure_matched,
        has_failure_context,
        ai_provider,
        max_tokens,
        timeout_ms,
        stderr_max_chars,
        has_warning,
        debounce_ms,
        last_ai_request_at,
        min_local_candidates,
        min_local_confidence,
    ) = {
        let mut state = state.lock().await;
        state.context.update_cwd(req.context.cwd.clone());

        let mut suggestions = Vec::new();

        let history_results = state.history.suggest(&req.input, &req.context.cwd, 5);
        suggestions.extend(history_results);

        let specs_results = state.specs.suggest(&req.input, req.cursor_pos);
        suggestions.extend(specs_results);

        let mut hint = None;
        let local_failure_matched = if let Some(exit_code) = req.context.last_exit_code
            && exit_code != 0
            && let Some(stderr) = &req.context.last_stderr
            && let Some((fail_suggestion, fail_hint)) =
                state.failure.match_failure(stderr, exit_code)
        {
            suggestions.push(fail_suggestion);
            hint = Some(fail_hint);
            true
        } else {
            false
        };

        let has_failure_context =
            req.context.last_exit_code.is_some_and(|c| c != 0) && req.context.last_stderr.is_some();

        let warning = if state.config.ui.risk_detection {
            state.risk.check(&req.input)
        } else {
            None
        };

        let has_warning = warning.is_some();
        let response = Arbitrator::arbitrate(suggestions, &req.context, hint, warning);
        let ai_provider = state.ai_provider.clone();
        let max_tokens = state.config.ai.max_tokens;
        let timeout_ms = state.config.ai.timeout_ms;
        let stderr_max_chars = state.config.context.stderr_max_chars;
        let debounce_ms = state.config.ai.debounce_ms;
        let last_ai_request_at = state.last_ai_request_at;
        let min_local_candidates = state.config.ai.min_local_candidates;
        let min_local_confidence = state.config.ai.min_local_confidence;

        (
            response,
            local_failure_matched,
            has_failure_context,
            ai_provider,
            max_tokens,
            timeout_ms,
            stderr_max_chars,
            has_warning,
            debounce_ms,
            last_ai_request_at,
            min_local_candidates,
            min_local_confidence,
        )
    };

    let local_candidate_count = response.suggestions.len();
    let max_local_confidence = response
        .suggestions
        .iter()
        .map(|s| s.confidence)
        .fold(0.0_f64, f64::max);

    let ai_useful = ai_is_useful(
        &req.input,
        has_warning,
        local_candidate_count,
        max_local_confidence,
        has_failure_context,
        local_failure_matched,
        min_local_candidates,
        min_local_confidence,
    );

    response.need_ai = ai_useful && ai_provider.is_some();

    if !req.skip_ai
        && let Some(provider) = ai_provider
        && should_request_ai(
            &req.input,
            has_warning,
            local_candidate_count,
            max_local_confidence,
            has_failure_context,
            local_failure_matched,
            last_ai_request_at,
            debounce_ms,
            min_local_candidates,
            min_local_confidence,
        )
    {
        {
            let mut state = state.lock().await;
            state.last_ai_request_at = Some(std::time::Instant::now());
        }

        let mut ctx = req.context.clone();
        crate::sanitize::sanitize_request_context(&mut ctx, stderr_max_chars);

        let is_recovery = has_failure_context && !local_failure_matched;
        if is_recovery {
            let prompt = ai::build_error_recovery_prompt(&ctx);
            tracing::info!("AI error recovery started, input_len={}", req.input.len());

            let ai_start = std::time::Instant::now();
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                provider.complete_nl(&prompt, max_tokens),
            )
            .await
            {
                Ok(Ok(ai_response)) => {
                    tracing::info!(
                        "AI error recovery received, latency_ms={}",
                        ai_start.elapsed().as_millis()
                    );
                    if let Some(suggestion) = ai::parse_nl_suggestion(&ai_response) {
                        Arbitrator::merge_ai_suggestion(&mut response, suggestion);
                    }
                }
                Ok(Err(e)) => tracing::info!("AI error recovery error: {e}"),
                Err(_) => tracing::info!(
                    "AI error recovery timed out, latency_ms={}",
                    ai_start.elapsed().as_millis()
                ),
            }
        } else {
            let prompt = ai::build_prompt(&req.input, &ctx);
            tracing::info!("AI completion started, input_len={}", req.input.len());

            let ai_start = std::time::Instant::now();
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                provider.complete(&prompt, max_tokens),
            )
            .await
            {
                Ok(Ok(ai_response)) => {
                    tracing::info!(
                        "AI completion received, latency_ms={}, response_len={}",
                        ai_start.elapsed().as_millis(),
                        ai_response.len()
                    );
                    if let Some(suggestion) = ai::parse_ai_suggestion(&req.input, &ai_response) {
                        tracing::info!("AI suggestion accepted, len={}", suggestion.text.len());
                        Arbitrator::merge_ai_suggestion(&mut response, suggestion);
                    } else {
                        tracing::info!("AI suggestion rejected by parser");
                    }
                }
                Ok(Err(e)) => {
                    tracing::info!("AI completion error: {e}");
                }
                Err(_) => {
                    tracing::info!(
                        "AI completion timed out, latency_ms={}",
                        ai_start.elapsed().as_millis()
                    );
                }
            }
        }
    }

    Response::Suggest(response)
}

async fn handle_nl_generate(req: NlGenerateRequest, state: &Arc<Mutex<DaemonState>>) -> Response {
    let (ai_provider, max_tokens, timeout_ms, stderr_max_chars, risk_enabled) = {
        let state = state.lock().await;
        (
            state.ai_provider.clone(),
            state.config.ai.max_tokens.max(200),
            state.config.ai.timeout_ms,
            state.config.context.stderr_max_chars,
            state.config.ui.risk_detection,
        )
    };

    let Some(provider) = ai_provider else {
        return Response::NlGenerate(NlGenerateResponse {
            command: None,
            explanation: None,
            warning: None,
        });
    };

    if req.query.is_empty() {
        return Response::NlGenerate(NlGenerateResponse {
            command: None,
            explanation: None,
            warning: None,
        });
    }

    let mut ctx = req.context.clone();
    crate::sanitize::sanitize_request_context(&mut ctx, stderr_max_chars);
    let prompt = ai::build_nl_prompt(&req.query, &ctx);
    tracing::info!("NL generation started, query_len={}", req.query.len());

    let ai_start = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        provider.complete_nl(&prompt, max_tokens),
    )
    .await
    {
        Ok(Ok(ai_response)) => {
            tracing::info!(
                "NL generation received, latency_ms={}, response_len={}",
                ai_start.elapsed().as_millis(),
                ai_response.len()
            );
            if let Some(suggestion) = ai::parse_nl_suggestion(&ai_response) {
                tracing::info!("NL suggestion accepted, len={}", suggestion.text.len());

                let warning = if risk_enabled {
                    let state = state.lock().await;
                    state.risk.check(&suggestion.text)
                } else {
                    None
                };

                return Response::NlGenerate(NlGenerateResponse {
                    command: Some(suggestion.text),
                    explanation: None,
                    warning,
                });
            }
        }
        Ok(Err(e)) => {
            tracing::info!("NL generation error: {e}");
        }
        Err(_) => {
            tracing::info!(
                "NL generation timed out, latency_ms={}",
                ai_start.elapsed().as_millis()
            );
        }
    }

    Response::NlGenerate(NlGenerateResponse {
        command: None,
        explanation: None,
        warning: None,
    })
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

    // ai_is_useful tests

    #[test]
    fn test_ai_is_useful_warning_blocks() {
        assert!(!ai_is_useful("git ch", true, 0, 0.0, false, false, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_error_recovery() {
        assert!(ai_is_useful("", false, 0, 0.0, true, false, 2, 0.6));
        assert!(ai_is_useful("gi", false, 0, 0.0, true, false, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_error_recovery_local_matched() {
        assert!(!ai_is_useful("", false, 1, 0.95, true, true, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_short_input_no_failure() {
        assert!(!ai_is_useful("g", false, 0, 0.0, false, false, 2, 0.6));
        assert!(!ai_is_useful("", false, 0, 0.0, false, false, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_sufficient_count() {
        assert!(!ai_is_useful("git ch", false, 3, 0.3, false, false, 2, 0.6));
        assert!(!ai_is_useful("git ch", false, 2, 0.1, false, false, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_sufficient_confidence() {
        assert!(!ai_is_useful("git ch", false, 1, 0.8, false, false, 2, 0.6));
        assert!(!ai_is_useful("git ch", false, 0, 0.6, false, false, 2, 0.6));
    }

    #[test]
    fn test_ai_is_useful_both_insufficient() {
        assert!(ai_is_useful("myapp d", false, 1, 0.3, false, false, 2, 0.6));
        assert!(ai_is_useful("myapp d", false, 0, 0.0, false, false, 2, 0.6));
        assert!(ai_is_useful(
            "myapp d", false, 1, 0.59, false, false, 2, 0.6
        ));
    }

    // should_request_ai tests

    #[test]
    fn test_should_request_ai_short_input() {
        assert!(!should_request_ai(
            "g", false, 0, 0.0, false, false, None, 300, 2, 0.6
        ));
        assert!(should_request_ai(
            "rm", false, 0, 0.0, false, false, None, 300, 2, 0.6
        ));
        assert!(should_request_ai(
            "gi", false, 0, 0.0, false, false, None, 300, 2, 0.6
        ));
    }

    #[test]
    fn test_should_request_ai_warning_present() {
        assert!(!should_request_ai(
            "rm -rf /", true, 0, 0.0, false, false, None, 300, 2, 0.6
        ));
    }

    #[test]
    fn test_should_request_ai_sufficient_local() {
        assert!(!should_request_ai(
            "git checkout",
            false,
            3,
            0.95,
            false,
            false,
            None,
            300,
            2,
            0.6
        ));
        assert!(!should_request_ai(
            "git checkout",
            false,
            2,
            0.7,
            false,
            false,
            None,
            300,
            2,
            0.6
        ));
        assert!(!should_request_ai(
            "git checkout",
            false,
            1,
            0.9,
            false,
            false,
            None,
            300,
            2,
            0.6
        ));
    }

    #[test]
    fn test_should_request_ai_debounce() {
        let recent = std::time::Instant::now();
        assert!(!should_request_ai(
            "docker run",
            false,
            0,
            0.0,
            false,
            false,
            Some(recent),
            300,
            2,
            0.6
        ));
    }

    #[test]
    fn test_should_request_ai_happy_path() {
        assert!(should_request_ai(
            "docker run",
            false,
            0,
            0.5,
            false,
            false,
            None,
            300,
            2,
            0.6
        ));
        assert!(should_request_ai(
            "myapp", false, 1, 0.3, false, false, None, 300, 2, 0.6
        ));
    }

    #[test]
    fn test_should_request_ai_sufficient_confidence_skips() {
        assert!(!should_request_ai(
            "git ch", false, 1, 0.9, false, false, None, 300, 2, 0.6
        ));
    }

    #[test]
    fn test_should_request_ai_error_recovery_bypasses_input_check() {
        assert!(should_request_ai(
            "", false, 0, 0.0, true, false, None, 300, 2, 0.6
        ));
    }
}
