use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AwenConfig {
    pub ai: AiConfig,
    pub context: ContextConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub provider: String,
    pub debounce_ms: u64,
    pub timeout_ms: u64,
    pub max_tokens: u32,
    pub cache_ttl_minutes: u32,
    pub deepseek: DeepSeekConfig,
    pub ollama: OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub model: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub session_history_size: usize,
    pub stderr_max_chars: usize,
    pub repo_detect: bool,
    pub git_context: bool,
    pub capture_stderr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub ghost_text_color: u8,
    pub dropdown_max_items: usize,
    pub hint_style: String,
    pub risk_detection: bool,
    pub command_explanation: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "deepseek".into(),
            debounce_ms: 300,
            timeout_ms: 30000,
            max_tokens: 1024,
            cache_ttl_minutes: 30,
            deepseek: DeepSeekConfig::default(),
            ollama: OllamaConfig::default(),
        }
    }
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "deepseek-chat".into(),
            base_url: "https://api.deepseek.com".into(),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            model: "qwen2.5-coder:7b".into(),
            base_url: "http://localhost:11434".into(),
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            session_history_size: 20,
            stderr_max_chars: 500,
            repo_detect: true,
            git_context: true,
            capture_stderr: false,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            ghost_text_color: 242,
            dropdown_max_items: 8,
            hint_style: "above".into(),
            risk_detection: true,
            command_explanation: false,
        }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        })
        .join("awen")
}

pub fn data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".local/share")
        })
        .join("awen")
}

pub fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

pub fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    runtime_dir().join(format!("awen-{uid}.sock"))
}

pub fn pid_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    runtime_dir().join(format!("awen-{uid}.pid"))
}

pub fn log_path() -> PathBuf {
    data_dir().join("awen.log")
}

pub fn history_db_path() -> PathBuf {
    data_dir().join("history.db")
}

pub fn load_config() -> AwenConfig {
    let path = config_dir().join("config.toml");
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    tracing::warn!("failed to parse config: {e}, using defaults");
                }
            },
            Err(e) => {
                tracing::warn!("failed to read config: {e}, using defaults");
            }
        }
    }
    AwenConfig::default()
}

pub fn load_config_from_str(s: &str) -> Result<AwenConfig, toml::de::Error> {
    toml::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AwenConfig::default();
        assert!(config.ai.enabled);
        assert_eq!(config.ai.provider, "deepseek");
        assert_eq!(config.ai.debounce_ms, 300);
        assert_eq!(config.context.session_history_size, 20);
        assert!(!config.context.capture_stderr);
        assert_eq!(config.ui.ghost_text_color, 242);
        assert!(!config.ui.command_explanation);
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_str = r#"
[ai]
enabled = false
provider = "ollama"
debounce_ms = 500
max_tokens = 100
cache_ttl_minutes = 60

[ai.ollama]
model = "codellama:7b"
base_url = "http://localhost:11434"

[context]
session_history_size = 30
stderr_max_chars = 1000
repo_detect = true
git_context = false
capture_stderr = true

[ui]
ghost_text_color = 240
dropdown_max_items = 10
hint_style = "below"
risk_detection = true
command_explanation = false
"#;
        let config: AwenConfig = load_config_from_str(toml_str).unwrap();
        assert!(!config.ai.enabled);
        assert_eq!(config.ai.provider, "ollama");
        assert_eq!(config.ai.debounce_ms, 500);
        assert_eq!(config.ai.ollama.model, "codellama:7b");
        assert_eq!(config.context.session_history_size, 30);
        assert!(!config.context.git_context);
        assert_eq!(config.ui.ghost_text_color, 240);
        assert!(!config.ui.command_explanation);
    }

    #[test]
    fn test_partial_config() {
        let toml_str = r#"
[ai]
enabled = false
"#;
        let config: AwenConfig = load_config_from_str(toml_str).unwrap();
        assert!(!config.ai.enabled);
        assert_eq!(config.ai.provider, "deepseek");
        assert_eq!(config.context.session_history_size, 20);
    }

    #[test]
    fn test_empty_config() {
        let config: AwenConfig = load_config_from_str("").unwrap();
        assert!(config.ai.enabled);
        assert_eq!(config.ai.provider, "deepseek");
    }

    #[test]
    fn test_config_serialization() {
        let config = AwenConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: AwenConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.ai.provider, parsed.ai.provider);
        assert_eq!(config.ui.ghost_text_color, parsed.ui.ghost_text_color);
    }
}
