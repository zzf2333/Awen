use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Suggest(SuggestRequest),
    Record(RecordCommandRequest),
    Status,
    Context,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestRequest {
    pub input: String,
    pub cursor_pos: usize,
    pub context: RequestContext,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub cwd: String,
    #[serde(default)]
    pub last_command: Option<String>,
    #[serde(default)]
    pub last_exit_code: Option<i32>,
    #[serde(default)]
    pub last_stderr: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_status: Option<String>,
    #[serde(default)]
    pub session_commands: Vec<String>,
    #[serde(default)]
    pub env_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordCommandRequest {
    pub command: String,
    pub exit_code: i32,
    #[serde(default)]
    pub stderr: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Suggest(SuggestResponse),
    Status(StatusResponse),
    Context(ContextResponse),
    Ok,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestResponse {
    pub suggestions: Vec<Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<Hint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<Warning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub text: String,
    pub source: SuggestionSource,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionSource {
    History,
    Specs,
    Ai,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hint {
    pub text: String,
    pub kind: HintKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    FailureRecovery,
    Explanation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub running: bool,
    pub pid: u32,
    pub uptime_secs: u64,
    pub history_count: u64,
    pub ai_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub recent_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_request_roundtrip() {
        let req = Request::Suggest(SuggestRequest {
            input: "docker run -".into(),
            cursor_pos: 12,
            context: RequestContext {
                cwd: "/home/user".into(),
                last_command: Some("docker build .".into()),
                last_exit_code: Some(0),
                last_stderr: None,
                git_branch: Some("main".into()),
                git_status: None,
                session_commands: vec!["npm run build".into()],
                env_hints: vec![],
            },
            timestamp: Some(1716100000),
        });
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::Suggest(s) => {
                assert_eq!(s.input, "docker run -");
                assert_eq!(s.cursor_pos, 12);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_suggest_response_roundtrip() {
        let resp = Response::Suggest(SuggestResponse {
            suggestions: vec![Suggestion {
                text: "it -p 3000:3000 myapp".into(),
                source: SuggestionSource::Ai,
                confidence: 0.92,
                description: Some("run recently built image".into()),
            }],
            hint: None,
            warning: Some(Warning {
                text: "This will delete everything".into(),
            }),
        });
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Suggest(s) => {
                assert_eq!(s.suggestions.len(), 1);
                assert!(s.warning.is_some());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_record_request_roundtrip() {
        let req = Request::Record(RecordCommandRequest {
            command: "cargo build".into(),
            exit_code: 1,
            stderr: Some("cannot find crate `tokio`".into()),
            cwd: "/home/user/project".into(),
            duration_ms: Some(3200),
        });
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::Record(r) => {
                assert_eq!(r.exit_code, 1);
                assert_eq!(r.command, "cargo build");
            }
            _ => panic!("wrong variant"),
        }
    }
}
