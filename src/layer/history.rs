use crate::protocol::{Suggestion, SuggestionSource};
use crate::sanitize::{SENSITIVE_COMMAND_RE, SENSITIVE_VALUE_RE};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAX_COMMAND_LENGTH: usize = 500;
const SUGGEST_QUERY_LIMIT: usize = 500;
const SUGGEST_NEXT_QUERY_LIMIT: usize = 200;
const SHORT_INPUT_MIN_SCORE: u32 = 50;
const LONG_INPUT_MIN_SCORE: u32 = 40;
const DIR_AFFINITY_SUGGEST: f64 = 1.5;
const DIR_AFFINITY_NEXT: f64 = 3.0;
const PREFIX_BONUS: f64 = 3.0;
const FAILURE_PENALTY: f64 = 0.3;
const LAST_CMD_PENALTY: f64 = 0.1;
const NEXT_CONFIDENCE_SCALE: f64 = 0.5;
const NEXT_CONFIDENCE_CAP: f64 = 0.5;
const SEQUENCE_BOOST_NEXT: f64 = 5.0;
const SEQUENCE_BOOST_SUGGEST: f64 = 2.0;
const SEQUENCE_QUERY_LIMIT: usize = 50;

pub fn is_sensitive_command(command: &str) -> bool {
    SENSITIVE_VALUE_RE.is_match(command) || SENSITIVE_COMMAND_RE.is_match(command)
}

pub fn normalize_command(command: &str) -> String {
    let mut words = command.split_whitespace();
    let first = match words.next() {
        Some(w) => w,
        None => return String::new(),
    };
    for word in words {
        if !word.starts_with('-') {
            return format!("{first} {word}");
        }
    }
    first.to_string()
}

pub struct HistoryLayer {
    db_path: PathBuf,
}

impl HistoryLayer {
    pub fn new(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS commands (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                exit_code INTEGER NOT NULL DEFAULT 0,
                count INTEGER NOT NULL DEFAULT 1,
                UNIQUE(command, cwd)
            );
            CREATE INDEX IF NOT EXISTS idx_commands_command ON commands(command);
            CREATE INDEX IF NOT EXISTS idx_commands_cwd ON commands(cwd);
            CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);
            CREATE TABLE IF NOT EXISTS command_sequences (
                prev_command TEXT NOT NULL,
                next_command TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 1,
                UNIQUE(prev_command, next_command)
            );
            CREATE INDEX IF NOT EXISTS idx_seq_prev ON command_sequences(prev_command);",
        )?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    pub fn record(&self, command: &str, cwd: &str, exit_code: i32) -> Result<(), rusqlite::Error> {
        let trimmed = command.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_COMMAND_LENGTH {
            return Ok(());
        }
        if is_sensitive_command(command) {
            tracing::debug!("skipping sensitive command in history");
            return Ok(());
        }
        let conn = Connection::open(&self.db_path)?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO commands (command, cwd, timestamp, exit_code, count)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(command, cwd) DO UPDATE SET
                count = count + 1,
                timestamp = ?3,
                exit_code = ?4",
            params![command, cwd, now, exit_code],
        )?;
        Ok(())
    }

    pub fn record_sequence(
        &self,
        prev_command: &str,
        next_command: &str,
    ) -> Result<(), rusqlite::Error> {
        let prev_norm = normalize_command(prev_command);
        let next_norm = normalize_command(next_command);
        if prev_norm.is_empty() || next_norm.is_empty() || prev_norm == next_norm {
            return Ok(());
        }
        if is_sensitive_command(prev_command) || is_sensitive_command(next_command) {
            return Ok(());
        }
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO command_sequences (prev_command, next_command, count)
             VALUES (?1, ?2, 1)
             ON CONFLICT(prev_command, next_command) DO UPDATE SET
                count = count + 1",
            params![prev_norm, next_norm],
        )?;
        Ok(())
    }

    fn query_sequence_boosts(
        &self,
        prev_command: &str,
        weight: f64,
    ) -> HashMap<String, f64> {
        let prev_norm = normalize_command(prev_command);
        if prev_norm.is_empty() {
            return HashMap::new();
        }
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT next_command, count FROM command_sequences
             WHERE prev_command = ?1 ORDER BY count DESC LIMIT ?2",
        ) else {
            return HashMap::new();
        };
        let rows: Vec<(String, i64)> = match stmt
            .query_map(params![prev_norm, SEQUENCE_QUERY_LIMIT as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            }) {
            Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
            Err(_) => return HashMap::new(),
        };
        let max_count = rows.first().map(|(_, c)| *c).unwrap_or(1).max(1) as f64;
        rows.into_iter()
            .map(|(cmd, count)| {
                let boost = 1.0 + weight * (count as f64 / max_count);
                (cmd, boost)
            })
            .collect()
    }

    pub fn suggest(
        &self,
        input: &str,
        cwd: &str,
        last_command: Option<&str>,
        limit: usize,
    ) -> Vec<Suggestion> {
        if input.is_empty() {
            return Vec::new();
        }

        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let query = format!(
            "SELECT command, cwd, timestamp, count, exit_code FROM commands
             ORDER BY timestamp DESC LIMIT {SUGGEST_QUERY_LIMIT}"
        );
        let mut stmt = conn.prepare(&query).unwrap();

        let rows: Vec<(String, String, i64, i64, i32)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i32>(4)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let seq_boosts = last_command
            .map(|lc| self.query_sequence_boosts(lc, SEQUENCE_BOOST_SUGGEST))
            .unwrap_or_default();

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(input, CaseMatching::Smart, Normalization::Smart);

        let now = chrono::Utc::now().timestamp();
        let mut scored: Vec<(f64, String)> = Vec::new();

        let input_lower = input.to_lowercase();
        let short_input = input.len() <= 3;
        let min_score: u32 = if short_input {
            SHORT_INPUT_MIN_SCORE
        } else {
            LONG_INPUT_MIN_SCORE
        };
        let input_first_word = input_lower.split_whitespace().next().unwrap_or("");
        let has_space = input.contains(' ');

        for (command, cmd_cwd, timestamp, count, exit_code) in &rows {
            if short_input && !command.to_lowercase().starts_with(&input_lower) {
                continue;
            }
            if has_space {
                let cmd_lower = command.to_lowercase();
                let cmd_first = cmd_lower.split_whitespace().next().unwrap_or("");
                if cmd_first != input_first_word {
                    continue;
                }
                if let Some(last_word) = input_lower.split_whitespace().last()
                    && last_word.starts_with('-')
                    && last_word.len() >= 3
                {
                    let has_flag = cmd_lower
                        .split_whitespace()
                        .any(|w| w.starts_with(last_word));
                    if !has_flag {
                        continue;
                    }
                }
            }
            let mut buf = Vec::new();
            let haystack = nucleo_matcher::Utf32Str::new(command, &mut buf);
            if let Some(match_score) = pattern.score(haystack, &mut matcher) {
                if match_score < min_score {
                    continue;
                }
                let age_hours = ((now - timestamp) as f64 / 3600.0).max(1.0);
                let recency_decay = 1.0 / age_hours.ln().max(1.0);
                let frequency_boost = (*count as f64).ln().max(1.0);
                let dir_affinity = if cmd_cwd == cwd {
                    DIR_AFFINITY_SUGGEST
                } else {
                    1.0
                };
                let prefix_bonus = if command.starts_with(input) {
                    PREFIX_BONUS
                } else {
                    1.0
                };
                let failure_penalty = if *exit_code != 0 {
                    FAILURE_PENALTY
                } else {
                    1.0
                };
                let last_cmd_penalty = if last_command.is_some_and(|lc| lc == command) {
                    LAST_CMD_PENALTY
                } else {
                    1.0
                };
                let seq_boost = seq_boosts
                    .get(&normalize_command(command))
                    .copied()
                    .unwrap_or(1.0);

                let score = match_score as f64
                    * recency_decay
                    * frequency_boost
                    * dir_affinity
                    * prefix_bonus
                    * failure_penalty
                    * last_cmd_penalty
                    * seq_boost;
                scored.push((score, command.clone()));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.dedup_by(|a, b| a.1 == b.1);

        let max_score = scored.first().map(|s| s.0).unwrap_or(1.0).max(1.0);

        scored
            .into_iter()
            .take(limit)
            .map(|(score, text)| Suggestion {
                text,
                source: SuggestionSource::History,
                confidence: (score / max_score).min(1.0),
                description: None,
            })
            .collect()
    }

    pub fn suggest_next(
        &self,
        cwd: &str,
        last_command: Option<&str>,
        limit: usize,
    ) -> Vec<Suggestion> {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let query = format!(
            "SELECT command, cwd, timestamp, count, exit_code FROM commands
             ORDER BY timestamp DESC LIMIT {SUGGEST_NEXT_QUERY_LIMIT}"
        );
        let mut stmt = conn.prepare(&query).unwrap();

        let rows: Vec<(String, String, i64, i64, i32)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i32>(4)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let seq_boosts = last_command
            .map(|lc| self.query_sequence_boosts(lc, SEQUENCE_BOOST_NEXT))
            .unwrap_or_default();

        let now = chrono::Utc::now().timestamp();
        let mut scored: Vec<(f64, String)> = Vec::new();

        for (command, cmd_cwd, timestamp, count, exit_code) in &rows {
            let age_hours = ((now - timestamp) as f64 / 3600.0).max(1.0);
            let recency = 1.0 / age_hours.ln().max(1.0);
            let frequency = (*count as f64).ln().max(1.0);
            let dir_affinity = if cmd_cwd == cwd {
                DIR_AFFINITY_NEXT
            } else {
                1.0
            };
            let failure_penalty = if *exit_code != 0 {
                FAILURE_PENALTY
            } else {
                1.0
            };
            let last_cmd_penalty = if last_command.is_some_and(|lc| lc == command) {
                LAST_CMD_PENALTY
            } else {
                1.0
            };
            let seq_boost = seq_boosts
                .get(&normalize_command(command))
                .copied()
                .unwrap_or(1.0);

            let score =
                recency * frequency * dir_affinity * failure_penalty * last_cmd_penalty * seq_boost;
            scored.push((score, command.clone()));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.dedup_by(|a, b| a.1 == b.1);

        let max_score = scored.first().map(|s| s.0).unwrap_or(1.0).max(1.0);

        scored
            .into_iter()
            .take(limit)
            .map(|(score, text)| Suggestion {
                text,
                source: SuggestionSource::History,
                confidence: (score / max_score * NEXT_CONFIDENCE_SCALE).min(NEXT_CONFIDENCE_CAP),
                description: None,
            })
            .collect()
    }

    pub fn count(&self) -> u64 {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, HistoryLayer) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("history.db");
        let layer = HistoryLayer::new(&db_path).unwrap();
        (dir, layer)
    }

    #[test]
    fn test_record_and_count() {
        let (_dir, layer) = setup();
        layer.record("ls -la", "/tmp", 0).unwrap();
        layer.record("pwd", "/tmp", 0).unwrap();
        assert_eq!(layer.count(), 2);
    }

    #[test]
    fn test_record_dedup() {
        let (_dir, layer) = setup();
        layer.record("ls -la", "/tmp", 0).unwrap();
        layer.record("ls -la", "/tmp", 0).unwrap();
        assert_eq!(layer.count(), 1);
    }

    #[test]
    fn test_suggest_prefix() {
        let (_dir, layer) = setup();
        layer.record("docker build .", "/app", 0).unwrap();
        layer
            .record("docker run -p 3000:3000 app", "/app", 0)
            .unwrap();
        layer.record("npm install", "/app", 0).unwrap();

        let results = layer.suggest("docker", "/app", None, 5);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .all(|s| s.source == SuggestionSource::History)
        );
    }

    #[test]
    fn test_suggest_empty_input() {
        let (_dir, layer) = setup();
        layer.record("ls", "/tmp", 0).unwrap();
        let results = layer.suggest("", "/tmp", None, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggest_directory_affinity() {
        let (_dir, layer) = setup();
        layer.record("make build", "/project-a", 0).unwrap();
        layer.record("make test", "/project-b", 0).unwrap();

        let results = layer.suggest("make", "/project-a", None, 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_is_sensitive_command_docker_login() {
        assert!(is_sensitive_command(
            "docker login registry.io -u user -p secret123"
        ));
    }

    #[test]
    fn test_is_sensitive_command_export_secret() {
        assert!(is_sensitive_command(
            "export API_KEY=sk-abcdefghijklmnopqrstuvwxyz12345"
        ));
        assert!(is_sensitive_command("export SECRET_TOKEN=abc123"));
    }

    #[test]
    fn test_is_sensitive_command_safe() {
        assert!(!is_sensitive_command("cargo build --release"));
        assert!(!is_sensitive_command("git status"));
        assert!(!is_sensitive_command("ls -la"));
        assert!(!is_sensitive_command("npm install express"));
    }

    #[test]
    fn test_record_skips_sensitive() {
        let (_dir, layer) = setup();
        layer
            .record("docker login registry.io -u user -p secret", "/tmp", 0)
            .unwrap();
        assert_eq!(layer.count(), 0);

        layer.record("cargo build", "/tmp", 0).unwrap();
        assert_eq!(layer.count(), 1);
    }

    #[test]
    fn test_suggest_next() {
        let (_dir, layer) = setup();
        layer.record("cargo build", "/project", 0).unwrap();
        layer.record("cargo test", "/project", 0).unwrap();
        layer.record("ls -la", "/other", 0).unwrap();

        let results = layer.suggest_next("/project", None, 5);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .all(|s| s.source == SuggestionSource::History)
        );
        assert!(results.iter().all(|s| s.confidence <= 0.5));
    }

    #[test]
    fn test_suggest_next_dir_affinity() {
        let (_dir, layer) = setup();
        layer.record("make build", "/project-a", 0).unwrap();
        layer.record("npm test", "/project-b", 0).unwrap();

        let results = layer.suggest_next("/project-a", None, 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "make build");
    }

    #[test]
    fn test_suggest_next_empty_db() {
        let (_dir, layer) = setup();
        let results = layer.suggest_next("/tmp", None, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggest_flag_prefix_filter() {
        let (_dir, layer) = setup();
        layer
            .record("claude --dangerously-skip-permissions", "/app", 0)
            .unwrap();
        layer
            .record("claude --print config", "/app", 0)
            .unwrap();

        let results = layer.suggest("claude --prin", "/app", None, 10);
        for s in &results {
            assert!(
                s.text.contains("--print"),
                "flag mismatch should be filtered: {}",
                s.text
            );
        }
    }

    #[test]
    fn test_suggest_short_flag_not_filtered() {
        let (_dir, layer) = setup();
        layer
            .record("claude --dangerously-skip-permissions", "/app", 0)
            .unwrap();

        let results = layer.suggest("claude --", "/app", None, 10);
        assert!(
            !results.is_empty(),
            "short flag prefix (--) should not filter"
        );
    }

    #[test]
    fn test_suggest_first_word_filter() {
        let (_dir, layer) = setup();
        layer.record("git add .", "/app", 0).unwrap();
        layer.record("git commit -m 'test'", "/app", 0).unwrap();
        layer
            .record("pkill -f daemon; sleep 1; source init", "/app", 0)
            .unwrap();
        layer.record("echo git add something", "/app", 0).unwrap();

        let results = layer.suggest("git a", "/app", None, 10);
        for s in &results {
            assert!(
                s.text.starts_with("git"),
                "expected git prefix, got: {}",
                s.text
            );
        }
    }

    #[test]
    fn test_normalize_command() {
        assert_eq!(normalize_command("git add ."), "git add");
        assert_eq!(normalize_command("git commit -m 'fix'"), "git commit");
        assert_eq!(normalize_command("cargo test --lib"), "cargo test");
        assert_eq!(normalize_command("ls -la"), "ls");
        assert_eq!(normalize_command("docker run -p 3000:3000 app"), "docker run");
        assert_eq!(normalize_command("npm install express"), "npm install");
        assert_eq!(normalize_command("pwd"), "pwd");
        assert_eq!(normalize_command(""), "");
        assert_eq!(normalize_command("  "), "");
    }

    #[test]
    fn test_record_sequence_basic() {
        let (_dir, layer) = setup();
        layer.record_sequence("git add .", "git commit -m 'test'").unwrap();

        let boosts = layer.query_sequence_boosts("git add .", SEQUENCE_BOOST_NEXT);
        assert!(boosts.contains_key("git commit"));
    }

    #[test]
    fn test_record_sequence_count_increment() {
        let (_dir, layer) = setup();
        for _ in 0..5 {
            layer.record_sequence("git add .", "git commit -m 'x'").unwrap();
        }
        layer.record_sequence("git add .", "git status").unwrap();

        let boosts = layer.query_sequence_boosts("git add", SEQUENCE_BOOST_NEXT);
        let commit_boost = boosts.get("git commit").copied().unwrap_or(1.0);
        let status_boost = boosts.get("git status").copied().unwrap_or(1.0);
        assert!(commit_boost > status_boost, "higher count should get higher boost");
    }

    #[test]
    fn test_record_sequence_sensitive_skipped() {
        let (_dir, layer) = setup();
        layer
            .record_sequence("export API_KEY=sk-abc123", "curl http://example.com")
            .unwrap();

        let boosts = layer.query_sequence_boosts("export API_KEY=sk-abc123", SEQUENCE_BOOST_NEXT);
        assert!(boosts.is_empty());
    }

    #[test]
    fn test_record_sequence_same_command_skipped() {
        let (_dir, layer) = setup();
        layer.record_sequence("ls -la", "ls -l").unwrap();

        let boosts = layer.query_sequence_boosts("ls", SEQUENCE_BOOST_NEXT);
        assert!(boosts.is_empty(), "same normalized command should be skipped");
    }

    #[test]
    fn test_suggest_next_with_sequence_boost() {
        let (_dir, layer) = setup();
        layer.record("git commit -m 'test'", "/app", 0).unwrap();
        layer.record("git status", "/app", 0).unwrap();
        layer.record("git push", "/app", 0).unwrap();

        for _ in 0..5 {
            layer.record_sequence("git add .", "git commit -m 'x'").unwrap();
        }

        let results = layer.suggest_next("/app", Some("git add ."), 5);
        assert!(!results.is_empty());
        assert_eq!(
            results[0].text, "git commit -m 'test'",
            "sequenced command should rank first"
        );
    }

    #[test]
    fn test_suggest_next_no_sequence_data() {
        let (_dir, layer) = setup();
        layer.record("cargo build", "/app", 0).unwrap();
        layer.record("cargo test", "/app", 0).unwrap();

        let results = layer.suggest_next("/app", Some("cargo fmt"), 5);
        assert!(!results.is_empty(), "should still return results without sequence data");
    }

    #[test]
    fn test_suggest_with_sequence_boost() {
        let (_dir, layer) = setup();
        layer.record("git commit -m 'test'", "/app", 0).unwrap();
        layer.record("git checkout main", "/app", 0).unwrap();

        for _ in 0..5 {
            layer.record_sequence("git add .", "git commit -m 'x'").unwrap();
        }

        let results = layer.suggest("git", "/app", Some("git add ."), 10);
        let commit_pos = results.iter().position(|s| s.text.contains("commit"));
        let checkout_pos = results.iter().position(|s| s.text.contains("checkout"));
        if let (Some(c), Some(co)) = (commit_pos, checkout_pos) {
            assert!(c < co, "sequenced git commit should rank above git checkout");
        }
    }
}
