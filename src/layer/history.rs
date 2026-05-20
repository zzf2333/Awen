use crate::protocol::{Suggestion, SuggestionSource};
use crate::sanitize::{SENSITIVE_COMMAND_RE, SENSITIVE_VALUE_RE};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

pub fn is_sensitive_command(command: &str) -> bool {
    SENSITIVE_VALUE_RE.is_match(command) || SENSITIVE_COMMAND_RE.is_match(command)
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
            CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);",
        )?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    pub fn record(&self, command: &str, cwd: &str, exit_code: i32) -> Result<(), rusqlite::Error> {
        let trimmed = command.trim();
        if trimmed.is_empty() || trimmed.len() > 500 {
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

    pub fn suggest(&self, input: &str, cwd: &str, limit: usize) -> Vec<Suggestion> {
        if input.is_empty() {
            return Vec::new();
        }

        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = conn
            .prepare(
                "SELECT command, cwd, timestamp, count FROM commands
                 ORDER BY timestamp DESC LIMIT 500",
            )
            .unwrap();

        let rows: Vec<(String, String, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(input, CaseMatching::Smart, Normalization::Smart);

        let now = chrono::Utc::now().timestamp();
        let mut scored: Vec<(f64, String)> = Vec::new();

        let input_lower = input.to_lowercase();
        let short_input = input.len() <= 3;
        let min_score: u32 = if short_input { 80 } else { 40 };
        let input_first_word = input_lower.split_whitespace().next().unwrap_or("");
        let has_space = input.contains(' ');

        for (command, cmd_cwd, timestamp, count) in &rows {
            if short_input && !command.to_lowercase().starts_with(&input_lower) {
                continue;
            }
            if has_space {
                let cmd_first_word = command.to_lowercase();
                let cmd_first = cmd_first_word.split_whitespace().next().unwrap_or("");
                if cmd_first != input_first_word {
                    continue;
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
                let dir_affinity = if cmd_cwd == cwd { 1.5 } else { 1.0 };
                let prefix_bonus = if command.starts_with(input) { 3.0 } else { 1.0 };

                let score = match_score as f64
                    * recency_decay
                    * frequency_boost
                    * dir_affinity
                    * prefix_bonus;
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

    pub fn suggest_next(&self, cwd: &str, limit: usize) -> Vec<Suggestion> {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stmt = conn
            .prepare(
                "SELECT command, cwd, timestamp, count FROM commands
                 ORDER BY timestamp DESC LIMIT 200",
            )
            .unwrap();

        let rows: Vec<(String, String, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let now = chrono::Utc::now().timestamp();
        let mut scored: Vec<(f64, String)> = Vec::new();

        for (command, cmd_cwd, timestamp, count) in &rows {
            let age_hours = ((now - timestamp) as f64 / 3600.0).max(1.0);
            let recency = 1.0 / age_hours.ln().max(1.0);
            let frequency = (*count as f64).ln().max(1.0);
            let dir_affinity = if cmd_cwd == cwd { 3.0 } else { 1.0 };

            let score = recency * frequency * dir_affinity;
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
                confidence: (score / max_score * 0.5).min(0.5),
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

        let results = layer.suggest("docker", "/app", 5);
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
        let results = layer.suggest("", "/tmp", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggest_directory_affinity() {
        let (_dir, layer) = setup();
        layer.record("make build", "/project-a", 0).unwrap();
        layer.record("make test", "/project-b", 0).unwrap();

        let results = layer.suggest("make", "/project-a", 5);
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

        let results = layer.suggest_next("/project", 5);
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

        let results = layer.suggest_next("/project-a", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "make build");
    }

    #[test]
    fn test_suggest_next_empty_db() {
        let (_dir, layer) = setup();
        let results = layer.suggest_next("/tmp", 5);
        assert!(results.is_empty());
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

        let results = layer.suggest("git a", "/app", 10);
        for s in &results {
            assert!(
                s.text.starts_with("git"),
                "expected git prefix, got: {}",
                s.text
            );
        }
    }
}
