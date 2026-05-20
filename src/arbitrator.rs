use crate::protocol::{
    Hint, RequestContext, SuggestResponse, Suggestion, SuggestionSource, Warning,
};

pub struct Arbitrator;

impl Arbitrator {
    pub fn arbitrate(
        mut suggestions: Vec<Suggestion>,
        context: &RequestContext,
        hint: Option<Hint>,
        warning: Option<Warning>,
    ) -> SuggestResponse {
        apply_context_weights(&mut suggestions, context);
        dedup_suggestions(&mut suggestions);
        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions.retain(|s| s.confidence >= 0.1);
        suggestions.truncate(8);

        SuggestResponse {
            suggestions,
            hint,
            warning,
        }
    }

    pub fn merge_ai_suggestion(response: &mut SuggestResponse, ai_suggestion: Suggestion) {
        if is_duplicate(&ai_suggestion, &response.suggestions) {
            return;
        }

        if response.suggestions.is_empty()
            || ai_suggestion.confidence > response.suggestions[0].confidence
        {
            response.suggestions.insert(0, ai_suggestion);
        } else {
            response.suggestions.push(ai_suggestion);
        }

        response.suggestions.truncate(8);
    }
}

fn apply_context_weights(suggestions: &mut [Suggestion], context: &RequestContext) {
    let is_failure = context.last_exit_code.is_some_and(|c| c != 0);
    let git_ahead = context
        .git_status
        .as_ref()
        .and_then(|s| {
            s.split(',')
                .find(|p| p.starts_with("ahead="))
                .and_then(|p| p.strip_prefix("ahead="))
                .and_then(|n| n.parse::<u32>().ok())
        })
        .unwrap_or(0);

    let input_tool = context
        .last_command
        .as_ref()
        .and_then(|c| c.split_whitespace().next())
        .unwrap_or("");

    for suggestion in suggestions.iter_mut() {
        if is_failure && suggestion.source == SuggestionSource::Failure {
            suggestion.confidence *= 3.0;
        }

        if git_ahead > 0 && suggestion.text.contains("push") {
            suggestion.confidence *= 2.0;
        }

        if suggestion.source == SuggestionSource::History {
            let same_tool = suggestion
                .text
                .split_whitespace()
                .next()
                .is_some_and(|t| t == input_tool);
            if same_tool && !input_tool.is_empty() {
                suggestion.confidence *= 1.3;
            }
        }
    }
}

fn dedup_suggestions(suggestions: &mut Vec<Suggestion>) {
    let mut i = 0;
    while i < suggestions.len() {
        let mut j = i + 1;
        while j < suggestions.len() {
            if is_similar(&suggestions[i].text, &suggestions[j].text) {
                if suggestions[i].confidence >= suggestions[j].confidence {
                    suggestions.remove(j);
                } else {
                    suggestions.remove(i);
                    j = i + 1;
                    continue;
                }
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

fn is_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let distance = strsim::levenshtein(a, b);
    distance <= 3 && (distance as f64) < (a.len().min(b.len()) as f64 * 0.3)
}

fn is_duplicate(new: &Suggestion, existing: &[Suggestion]) -> bool {
    existing.iter().any(|s| is_similar(&s.text, &new.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SuggestionSource;

    fn make_suggestion(text: &str, source: SuggestionSource, confidence: f64) -> Suggestion {
        Suggestion {
            text: text.into(),
            source,
            confidence,
            description: None,
        }
    }

    fn default_context() -> RequestContext {
        RequestContext {
            cwd: "/tmp".into(),
            last_command: None,
            last_exit_code: Some(0),
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        }
    }

    #[test]
    fn test_arbitrate_sorts_by_confidence() {
        let suggestions = vec![
            make_suggestion("ls -la", SuggestionSource::History, 0.5),
            make_suggestion("ls -lah", SuggestionSource::Specs, 0.9),
            make_suggestion("ls -l", SuggestionSource::Ai, 0.7),
        ];

        let result = Arbitrator::arbitrate(suggestions, &default_context(), None, None);
        assert_eq!(result.suggestions[0].text, "ls -lah");
        assert_eq!(result.suggestions[1].text, "ls -l");
    }

    #[test]
    fn test_dedup() {
        let suggestions = vec![
            make_suggestion("docker run", SuggestionSource::History, 0.8),
            make_suggestion("docker run", SuggestionSource::Ai, 0.6),
        ];

        let result = Arbitrator::arbitrate(suggestions, &default_context(), None, None);
        assert_eq!(result.suggestions.len(), 1);
        assert_eq!(result.suggestions[0].confidence, 0.8);
    }

    #[test]
    fn test_failure_weight_boost() {
        let suggestions = vec![
            make_suggestion("ls -la", SuggestionSource::History, 0.8),
            make_suggestion("cargo add tokio", SuggestionSource::Failure, 0.5),
        ];

        let mut ctx = default_context();
        ctx.last_exit_code = Some(1);

        let result = Arbitrator::arbitrate(suggestions, &ctx, None, None);
        assert_eq!(result.suggestions[0].source, SuggestionSource::Failure);
    }

    #[test]
    fn test_git_push_weight() {
        let suggestions = vec![
            make_suggestion("git status", SuggestionSource::History, 0.8),
            make_suggestion("git push", SuggestionSource::History, 0.7),
        ];

        let mut ctx = default_context();
        ctx.git_status = Some("ahead=3".into());

        let result = Arbitrator::arbitrate(suggestions, &ctx, None, None);
        assert_eq!(result.suggestions[0].text, "git push");
    }

    #[test]
    fn test_merge_ai_suggestion() {
        let mut response = SuggestResponse {
            suggestions: vec![make_suggestion("ls -la", SuggestionSource::History, 0.8)],
            hint: None,
            warning: None,
        };

        let ai = make_suggestion("ls -la --color", SuggestionSource::Ai, 0.9);
        Arbitrator::merge_ai_suggestion(&mut response, ai);

        assert_eq!(response.suggestions.len(), 2);
        assert_eq!(response.suggestions[0].source, SuggestionSource::Ai);
    }

    #[test]
    fn test_merge_ai_duplicate() {
        let mut response = SuggestResponse {
            suggestions: vec![make_suggestion(
                "docker run",
                SuggestionSource::History,
                0.8,
            )],
            hint: None,
            warning: None,
        };

        let ai = make_suggestion("docker run", SuggestionSource::Ai, 0.9);
        Arbitrator::merge_ai_suggestion(&mut response, ai);

        assert_eq!(response.suggestions.len(), 1);
    }

    #[test]
    fn test_is_similar() {
        assert!(is_similar("docker run", "docker run"));
        assert!(is_similar("docker run", "docker rn"));
        assert!(!is_similar("docker run", "npm install"));
    }
}
