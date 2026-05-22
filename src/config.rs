use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AwenConfig {
    pub ai: AiConfig,
    pub context: ContextConfig,
    pub ui: UiConfig,
    pub filesystem: FilesystemConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub debounce_ms: u64,
    pub timeout_ms: u64,
    pub max_tokens: u32,
    pub cache_ttl_minutes: u32,
    pub min_local_candidates: usize,
    pub min_local_confidence: f64,
    pub features: AiFeaturesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiFeaturesConfig {
    pub error_recovery: bool,
    pub completion: bool,
    pub nl_generation: bool,
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
    pub mode: UiMode,
    pub ghost_text_color: u8,
    pub dropdown_max_items: usize,
    pub hint_style: String,
    pub risk_detection: bool,
    pub command_explanation: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    #[default]
    Minimal,
    Full,
}

impl std::fmt::Display for UiMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiMode::Minimal => write!(f, "minimal"),
            UiMode::Full => write!(f, "full"),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
            api_key: String::new(),
            debounce_ms: 300,
            timeout_ms: 30000,
            max_tokens: 1024,
            cache_ttl_minutes: 30,
            min_local_candidates: 2,
            min_local_confidence: 0.6,
            features: AiFeaturesConfig::default(),
        }
    }
}

impl Default for AiFeaturesConfig {
    fn default() -> Self {
        Self {
            error_recovery: true,
            completion: false,
            nl_generation: false,
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
            mode: UiMode::default(),
            ghost_text_color: 242,
            dropdown_max_items: 8,
            hint_style: "above".into(),
            risk_detection: true,
            command_explanation: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesystemConfig {
    pub enabled: bool,
    pub cache_ttl_ms: u64,
    pub max_scan_entries: usize,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl_ms: 2000,
            max_scan_entries: 1000,
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

pub fn default_zsh_histfile() -> PathBuf {
    if let Ok(histfile) = std::env::var("HISTFILE") {
        return PathBuf::from(histfile);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".zsh_history")
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
        assert!(!config.ai.enabled);
        assert_eq!(config.ai.base_url, "https://api.deepseek.com");
        assert_eq!(config.ai.model, "deepseek-chat");
        assert!(config.ai.api_key.is_empty());
        assert_eq!(config.ai.debounce_ms, 300);
        assert_eq!(config.ai.min_local_candidates, 2);
        assert!((config.ai.min_local_confidence - 0.6).abs() < f64::EPSILON);
        assert!(config.ai.features.error_recovery);
        assert!(!config.ai.features.completion);
        assert!(!config.ai.features.nl_generation);
        assert_eq!(config.context.session_history_size, 20);
        assert!(!config.context.capture_stderr);
        assert_eq!(config.ui.mode, UiMode::Minimal);
        assert_eq!(config.ui.ghost_text_color, 242);
        assert!(!config.ui.command_explanation);
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_str = r#"
[ai]
enabled = false
base_url = "http://localhost:11434/v1"
model = "codellama:7b"
debounce_ms = 500
max_tokens = 100
cache_ttl_minutes = 60

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
        assert_eq!(config.ai.base_url, "http://localhost:11434/v1");
        assert_eq!(config.ai.model, "codellama:7b");
        assert_eq!(config.ai.debounce_ms, 500);
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
        assert_eq!(config.ai.base_url, "https://api.deepseek.com");
        assert_eq!(config.context.session_history_size, 20);
    }

    #[test]
    fn test_empty_config() {
        let config: AwenConfig = load_config_from_str("").unwrap();
        assert!(!config.ai.enabled);
        assert_eq!(config.ai.model, "deepseek-chat");
    }

    #[test]
    fn test_config_serialization() {
        let config = AwenConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: AwenConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.ai.base_url, parsed.ai.base_url);
        assert_eq!(config.ai.model, parsed.ai.model);
        assert_eq!(config.ui.ghost_text_color, parsed.ui.ghost_text_color);
    }
}
