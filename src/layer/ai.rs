use std::sync::Arc;

use crate::config::AwenConfig;
use crate::protocol::{RequestContext, Suggestion, SuggestionSource};

#[async_trait::async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, AiError>;
}

#[derive(Debug)]
pub enum AiError {
    Disabled,
    NoApiKey,
    RequestFailed(String),
    ParseFailed(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Disabled => write!(f, "AI is disabled"),
            AiError::NoApiKey => write!(f, "no API key configured"),
            AiError::RequestFailed(e) => write!(f, "request failed: {e}"),
            AiError::ParseFailed(e) => write!(f, "parse failed: {e}"),
        }
    }
}

impl std::error::Error for AiError {}

pub struct DeepSeekProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl DeepSeekProvider {
    pub fn new(config: &AwenConfig) -> Result<Self, AiError> {
        let api_key = if config.ai.deepseek.api_key.is_empty() {
            std::env::var("DEEPSEEK_API_KEY").map_err(|_| AiError::NoApiKey)?
        } else {
            config.ai.deepseek.api_key.clone()
        };

        Ok(Self {
            api_key,
            model: config.ai.deepseek.model.clone(),
            base_url: config.ai.deepseek.base_url.clone(),
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait::async_trait]
impl AiProvider for DeepSeekProvider {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, AiError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a terminal command completion assistant. Given the context and partial input, suggest the most likely command completion. Reply with ONLY the completion text (the part after what the user already typed), nothing else. No explanation, no markdown, no quotes."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": max_tokens,
            "temperature": 0.1,
            "stream": false
        });

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AiError::ParseFailed(e.to_string()))?;

        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| AiError::ParseFailed("no content in response".into()))
    }
}

pub struct OllamaProvider {
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(config: &AwenConfig) -> Self {
        Self {
            model: config.ai.ollama.model.clone(),
            base_url: config.ai.ollama.base_url.clone(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl AiProvider for OllamaProvider {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, AiError> {
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "num_predict": max_tokens,
                "temperature": 0.1
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AiError::ParseFailed(e.to_string()))?;

        json["response"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| AiError::ParseFailed("no response field".into()))
    }
}

pub fn build_prompt(input: &str, context: &RequestContext) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Working directory: {}", context.cwd));

    if let Some(branch) = &context.git_branch {
        parts.push(format!("Git branch: {branch}"));
    }
    if let Some(status) = &context.git_status {
        parts.push(format!("Git status: {status}"));
    }

    if !context.session_commands.is_empty() {
        let recent: Vec<&str> = context
            .session_commands
            .iter()
            .rev()
            .take(5)
            .map(|s| s.as_str())
            .collect();
        parts.push(format!("Recent commands: {}", recent.join(" → ")));
    }

    if let Some(code) = context.last_exit_code
        && code != 0
    {
        parts.push(format!("Last command failed (exit code {code})"));
        if let Some(stderr) = &context.last_stderr {
            parts.push(format!("Error: {stderr}"));
        }
    }

    if !context.env_hints.is_empty() {
        parts.push(format!("Environment: {}", context.env_hints.join(", ")));
    }

    parts.push(format!("Current input: {input}"));
    parts.push("Complete this command:".into());

    parts.join("\n")
}

pub fn create_provider(config: &AwenConfig) -> Option<Arc<dyn AiProvider>> {
    if !config.ai.enabled {
        return None;
    }

    match config.ai.provider.as_str() {
        "deepseek" => match DeepSeekProvider::new(config) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                tracing::warn!("failed to create DeepSeek provider: {e}");
                None
            }
        },
        "ollama" => Some(Arc::new(OllamaProvider::new(config))),
        other => {
            tracing::warn!("unknown AI provider: {other}");
            None
        }
    }
}

pub fn parse_ai_suggestion(_input: &str, ai_response: &str) -> Option<Suggestion> {
    let text = ai_response.trim();
    if text.is_empty() || text.len() > 200 {
        return None;
    }
    Some(Suggestion {
        text: text.to_string(),
        source: SuggestionSource::Ai,
        confidence: 0.7,
        description: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt() {
        let ctx = RequestContext {
            cwd: "/home/user/project".into(),
            last_command: Some("cargo build".into()),
            last_exit_code: Some(1),
            last_stderr: Some("cannot find crate `tokio`".into()),
            git_branch: Some("feat/auth".into()),
            git_status: Some("ahead=2".into()),
            session_commands: vec!["npm run build".into(), "cargo build".into()],
            env_hints: vec!["RUST_LOG=debug".into()],
        };

        let prompt = build_prompt("cargo ", &ctx);
        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("feat/auth"));
        assert!(prompt.contains("cargo "));
    }

    #[test]
    fn test_parse_ai_suggestion_valid() {
        let result = parse_ai_suggestion("docker ", "run -p 3000:3000 myapp");
        assert!(result.is_some());
        let s = result.unwrap();
        assert_eq!(s.text, "run -p 3000:3000 myapp");
        assert_eq!(s.source, SuggestionSource::Ai);
    }

    #[test]
    fn test_parse_ai_suggestion_empty() {
        assert!(parse_ai_suggestion("docker ", "").is_none());
        assert!(parse_ai_suggestion("docker ", "   ").is_none());
    }

    #[test]
    fn test_parse_ai_suggestion_too_long() {
        let long = "a".repeat(201);
        assert!(parse_ai_suggestion("docker ", &long).is_none());
    }

    #[test]
    fn test_create_provider_disabled() {
        let mut config = AwenConfig::default();
        config.ai.enabled = false;
        assert!(create_provider(&config).is_none());
    }
}
