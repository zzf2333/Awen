use std::sync::Arc;
use std::time::Instant;

use crate::arbitrator::Arbitrator;
use crate::config::AiConfig;
use crate::layer::ai::{self, AiProvider};
use crate::protocol::{RequestContext, SuggestResponse, Suggestion};

pub struct LocalResult {
    pub response: SuggestResponse,
    pub has_failure_context: bool,
    pub local_failure_matched: bool,
}

pub struct AiParams {
    pub provider: Arc<dyn AiProvider>,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    pub stderr_max_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDecision {
    RequestNow,
    NeedAiOnly,
    Skip,
}

pub struct AiTriggerPolicy {
    min_local_candidates: usize,
    min_local_confidence: f64,
    debounce_ms: u64,
}

impl AiTriggerPolicy {
    pub fn new(config: &AiConfig) -> Self {
        Self {
            min_local_candidates: config.min_local_candidates,
            min_local_confidence: config.min_local_confidence,
            debounce_ms: config.debounce_ms,
        }
    }

    pub fn evaluate(
        &self,
        input: &str,
        local: &LocalResult,
        ai_available: bool,
        skip_ai: bool,
        last_ai_request_at: Option<Instant>,
    ) -> AiDecision {
        if !self.ai_is_useful(input, local) {
            return AiDecision::Skip;
        }
        if !ai_available {
            return AiDecision::Skip;
        }
        if skip_ai {
            return AiDecision::NeedAiOnly;
        }
        if let Some(last_at) = last_ai_request_at
            && (last_at.elapsed().as_millis() as u64) < self.debounce_ms
        {
            return AiDecision::NeedAiOnly;
        }
        AiDecision::RequestNow
    }

    fn ai_is_useful(&self, input: &str, local: &LocalResult) -> bool {
        if local.response.warning.is_some() {
            return false;
        }
        if local.has_failure_context && !local.local_failure_matched {
            return true;
        }
        if input.len() < 2 {
            return false;
        }
        let candidate_count = local.response.suggestions.len();
        let max_confidence = local
            .response
            .suggestions
            .iter()
            .map(|s| s.confidence)
            .fold(0.0_f64, f64::max);
        candidate_count < self.min_local_candidates && max_confidence < self.min_local_confidence
    }
}

enum AiTask<'a> {
    ErrorRecovery {
        context: &'a RequestContext,
    },
    Completion {
        input: &'a str,
        context: &'a RequestContext,
    },
}

pub struct SuggestionPipeline;

impl SuggestionPipeline {
    pub async fn execute(
        mut local: LocalResult,
        decision: AiDecision,
        ai_params: Option<AiParams>,
        input: &str,
        context: &RequestContext,
    ) -> SuggestResponse {
        local.response.need_ai =
            matches!(decision, AiDecision::RequestNow | AiDecision::NeedAiOnly);

        if decision == AiDecision::RequestNow
            && let Some(ref params) = ai_params
        {
            let task = if local.has_failure_context && !local.local_failure_matched {
                AiTask::ErrorRecovery { context }
            } else {
                AiTask::Completion { input, context }
            };

            if let Some(suggestion) = Self::execute_ai(task, params).await {
                Arbitrator::merge_ai_suggestion(&mut local.response, suggestion);
            }
        }

        local.response
    }

    async fn execute_ai(task: AiTask<'_>, params: &AiParams) -> Option<Suggestion> {
        let mut ctx = match &task {
            AiTask::ErrorRecovery { context } => (*context).clone(),
            AiTask::Completion { context, .. } => (*context).clone(),
        };
        crate::sanitize::sanitize_request_context(&mut ctx, params.stderr_max_chars);

        let (prompt, label) = match &task {
            AiTask::ErrorRecovery { .. } => {
                (ai::build_error_recovery_prompt(&ctx), "error recovery")
            }
            AiTask::Completion { input, .. } => (ai::build_prompt(input, &ctx), "completion"),
        };

        tracing::info!("AI {label} started");
        let start = Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(params.timeout_ms),
            match &task {
                AiTask::ErrorRecovery { .. } => {
                    params.provider.complete_nl(&prompt, params.max_tokens)
                }
                AiTask::Completion { .. } => params.provider.complete(&prompt, params.max_tokens),
            },
        )
        .await;

        match result {
            Ok(Ok(ai_response)) => {
                tracing::info!(
                    "AI {label} received, latency_ms={}",
                    start.elapsed().as_millis()
                );
                match &task {
                    AiTask::ErrorRecovery { .. } => ai::parse_nl_suggestion(&ai_response),
                    AiTask::Completion { input, .. } => {
                        let suggestion = ai::parse_ai_suggestion(input, &ai_response);
                        if suggestion.is_some() {
                            tracing::info!("AI suggestion accepted");
                        } else {
                            tracing::info!("AI suggestion rejected by parser");
                        }
                        suggestion
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::info!("AI {label} error: {e}");
                None
            }
            Err(_) => {
                tracing::info!(
                    "AI {label} timed out, latency_ms={}",
                    start.elapsed().as_millis()
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SuggestResponse, Suggestion, SuggestionSource, Warning};

    fn make_local(
        candidate_count: usize,
        max_confidence: f64,
        has_warning: bool,
        has_failure_context: bool,
        local_failure_matched: bool,
    ) -> LocalResult {
        let suggestions: Vec<Suggestion> = (0..candidate_count)
            .map(|i| {
                let confidence = if i == 0 { max_confidence } else { 0.0 };
                Suggestion {
                    text: format!("cmd{i}"),
                    source: SuggestionSource::History,
                    confidence,
                    description: None,
                }
            })
            .collect();
        let warning = if has_warning {
            Some(Warning {
                text: "dangerous".into(),
            })
        } else {
            None
        };
        LocalResult {
            response: SuggestResponse {
                suggestions,
                hint: None,
                warning,
                need_ai: false,
            },
            has_failure_context,
            local_failure_matched,
        }
    }

    fn policy() -> AiTriggerPolicy {
        AiTriggerPolicy {
            min_local_candidates: 2,
            min_local_confidence: 0.6,
            debounce_ms: 300,
        }
    }

    // --- ai_is_useful tests (ported from daemon.rs) ---

    #[test]
    fn warning_blocks_ai() {
        let p = policy();
        let local = make_local(0, 0.0, true, false, false);
        assert_eq!(
            p.evaluate("git ch", &local, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn error_recovery_triggers_ai() {
        let p = policy();
        let local = make_local(0, 0.0, false, true, false);
        assert_eq!(
            p.evaluate("", &local, true, false, None),
            AiDecision::RequestNow
        );
        assert_eq!(
            p.evaluate("gi", &local, true, false, None),
            AiDecision::RequestNow
        );
    }

    #[test]
    fn error_recovery_local_matched_skips() {
        let p = policy();
        let local = make_local(1, 0.95, false, true, true);
        assert_eq!(p.evaluate("", &local, true, false, None), AiDecision::Skip);
    }

    #[test]
    fn short_input_no_failure_skips() {
        let p = policy();
        let local = make_local(0, 0.0, false, false, false);
        assert_eq!(p.evaluate("g", &local, true, false, None), AiDecision::Skip);
        assert_eq!(p.evaluate("", &local, true, false, None), AiDecision::Skip);
    }

    #[test]
    fn sufficient_count_skips() {
        let p = policy();
        let local1 = make_local(3, 0.3, false, false, false);
        assert_eq!(
            p.evaluate("git ch", &local1, true, false, None),
            AiDecision::Skip
        );
        let local2 = make_local(2, 0.1, false, false, false);
        assert_eq!(
            p.evaluate("git ch", &local2, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn sufficient_confidence_skips() {
        let p = policy();
        let local1 = make_local(1, 0.8, false, false, false);
        assert_eq!(
            p.evaluate("git ch", &local1, true, false, None),
            AiDecision::Skip
        );
        let local2 = make_local(1, 0.6, false, false, false);
        assert_eq!(
            p.evaluate("git ch", &local2, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn both_insufficient_triggers_ai() {
        let p = policy();
        let local1 = make_local(1, 0.3, false, false, false);
        assert_eq!(
            p.evaluate("myapp d", &local1, true, false, None),
            AiDecision::RequestNow
        );
        let local2 = make_local(0, 0.0, false, false, false);
        assert_eq!(
            p.evaluate("myapp d", &local2, true, false, None),
            AiDecision::RequestNow
        );
        let local3 = make_local(1, 0.59, false, false, false);
        assert_eq!(
            p.evaluate("myapp d", &local3, true, false, None),
            AiDecision::RequestNow
        );
    }

    // --- should_request_ai tests (ported from daemon.rs) ---

    #[test]
    fn short_input_request_behavior() {
        let p = policy();
        let local = make_local(0, 0.0, false, false, false);
        assert_eq!(p.evaluate("g", &local, true, false, None), AiDecision::Skip);
        assert_eq!(
            p.evaluate("rm", &local, true, false, None),
            AiDecision::RequestNow
        );
        assert_eq!(
            p.evaluate("gi", &local, true, false, None),
            AiDecision::RequestNow
        );
    }

    #[test]
    fn warning_blocks_request() {
        let p = policy();
        let local = make_local(0, 0.0, true, false, false);
        assert_eq!(
            p.evaluate("rm -rf /", &local, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn sufficient_local_blocks_request() {
        let p = policy();
        let local1 = make_local(3, 0.95, false, false, false);
        assert_eq!(
            p.evaluate("git checkout", &local1, true, false, None),
            AiDecision::Skip
        );
        let local2 = make_local(2, 0.7, false, false, false);
        assert_eq!(
            p.evaluate("git checkout", &local2, true, false, None),
            AiDecision::Skip
        );
        let local3 = make_local(1, 0.9, false, false, false);
        assert_eq!(
            p.evaluate("git checkout", &local3, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn debounce_returns_need_ai_only() {
        let p = policy();
        let local = make_local(0, 0.0, false, false, false);
        let recent = Instant::now();
        assert_eq!(
            p.evaluate("docker run", &local, true, false, Some(recent)),
            AiDecision::NeedAiOnly
        );
    }

    #[test]
    fn happy_path_requests_ai() {
        let p = policy();
        let local1 = make_local(0, 0.5, false, false, false);
        assert_eq!(
            p.evaluate("docker run", &local1, true, false, None),
            AiDecision::RequestNow
        );
        let local2 = make_local(1, 0.3, false, false, false);
        assert_eq!(
            p.evaluate("myapp", &local2, true, false, None),
            AiDecision::RequestNow
        );
    }

    #[test]
    fn sufficient_confidence_blocks_request() {
        let p = policy();
        let local = make_local(1, 0.9, false, false, false);
        assert_eq!(
            p.evaluate("git ch", &local, true, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn error_recovery_bypasses_input_check() {
        let p = policy();
        let local = make_local(0, 0.0, false, true, false);
        assert_eq!(
            p.evaluate("", &local, true, false, None),
            AiDecision::RequestNow
        );
    }

    // --- New tests for evaluate edge cases ---

    #[test]
    fn skip_ai_returns_need_ai_only() {
        let p = policy();
        let local = make_local(0, 0.0, false, false, false);
        assert_eq!(
            p.evaluate("docker run", &local, true, true, None),
            AiDecision::NeedAiOnly
        );
    }

    #[test]
    fn no_ai_provider_skips() {
        let p = policy();
        let local = make_local(0, 0.0, false, false, false);
        assert_eq!(
            p.evaluate("docker run", &local, false, false, None),
            AiDecision::Skip
        );
    }

    #[test]
    fn error_recovery_with_skip_ai_returns_need_ai_only() {
        let p = policy();
        let local = make_local(0, 0.0, false, true, false);
        assert_eq!(
            p.evaluate("", &local, true, true, None),
            AiDecision::NeedAiOnly
        );
    }
}
