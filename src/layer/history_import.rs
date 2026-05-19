use crate::layer::history::is_sensitive_command;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct ImportResult {
    pub total_lines: usize,
    pub imported: usize,
    pub skipped_sensitive: usize,
    pub skipped_empty: usize,
}

struct ParsedEntry {
    command: String,
    timestamp: Option<i64>,
}

fn parse_extended_history_line(line: &str) -> Option<ParsedEntry> {
    let rest = line.strip_prefix(": ")?;
    let colon_pos = rest.find(':')?;
    let timestamp: i64 = rest[..colon_pos].parse().ok()?;
    let after_colon = &rest[colon_pos + 1..];
    let semicolon_pos = after_colon.find(';')?;
    let command = &after_colon[semicolon_pos + 1..];
    Some(ParsedEntry {
        command: command.to_string(),
        timestamp: Some(timestamp),
    })
}

fn parse_zsh_history(content: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let mut current_command = String::new();
    let mut current_timestamp: Option<i64> = None;

    for line in content.lines() {
        if !current_command.is_empty() {
            current_command.push('\n');
            if let Some(stripped) = line.strip_suffix('\\') {
                current_command.push_str(stripped);
                continue;
            }
            current_command.push_str(line);
            entries.push(ParsedEntry {
                command: std::mem::take(&mut current_command),
                timestamp: current_timestamp.take(),
            });
            continue;
        }

        if let Some(entry) = parse_extended_history_line(line) {
            if let Some(stripped) = entry.command.strip_suffix('\\') {
                current_command = stripped.to_string();
                current_timestamp = entry.timestamp;
                continue;
            }
            entries.push(entry);
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Some(stripped) = trimmed.strip_suffix('\\') {
                    current_command = stripped.to_string();
                    continue;
                }
                entries.push(ParsedEntry {
                    command: trimmed.to_string(),
                    timestamp: None,
                });
            }
        }
    }

    if !current_command.is_empty() {
        entries.push(ParsedEntry {
            command: current_command,
            timestamp: current_timestamp,
        });
    }

    entries
}

pub fn import_zsh_history(
    db_path: &Path,
    history_path: &Path,
) -> Result<ImportResult, Box<dyn std::error::Error + Send + Sync>> {
    let raw_bytes = std::fs::read(history_path)?;
    let content = String::from_utf8_lossy(&raw_bytes);
    let entries = parse_zsh_history(&content);

    let mut result = ImportResult {
        total_lines: entries.len(),
        imported: 0,
        skipped_sensitive: 0,
        skipped_empty: 0,
    };

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

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO commands (command, cwd, timestamp, exit_code, count)
             VALUES (?1, ?2, ?3, 0, 1)
             ON CONFLICT(command, cwd) DO UPDATE SET
                count = count + 1,
                timestamp = MAX(timestamp, ?3)",
        )?;

        let fallback_ts = chrono::Utc::now().timestamp();

        for entry in &entries {
            let command = entry.command.trim();
            if command.is_empty() {
                result.skipped_empty += 1;
                continue;
            }
            if is_sensitive_command(command) {
                result.skipped_sensitive += 1;
                continue;
            }
            let ts = entry.timestamp.unwrap_or(fallback_ts);
            stmt.execute(params![command, "", ts])?;
            result.imported += 1;
        }
    }
    tx.commit()?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_plain_format() {
        let content = "ls -la\ngit status\ncargo build\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].command, "ls -la");
        assert_eq!(entries[1].command, "git status");
        assert_eq!(entries[2].command, "cargo build");
        assert!(entries[0].timestamp.is_none());
    }

    #[test]
    fn test_parse_extended_format() {
        let content = ": 1716100000:0;ls -la\n: 1716100001:0;git status\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "ls -la");
        assert_eq!(entries[0].timestamp, Some(1716100000));
        assert_eq!(entries[1].command, "git status");
        assert_eq!(entries[1].timestamp, Some(1716100001));
    }

    #[test]
    fn test_parse_multiline_command() {
        let content = ": 1716100002:0;docker run \\\n-p 3000:3000 \\\nmyapp\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "docker run \n-p 3000:3000 \nmyapp");
        assert_eq!(entries[0].timestamp, Some(1716100002));
    }

    #[test]
    fn test_parse_multiline_plain() {
        let content = "docker run \\\n-p 3000:3000 \\\nmyapp\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "docker run \n-p 3000:3000 \nmyapp");
    }

    #[test]
    fn test_parse_mixed_format() {
        let content = "ls\n: 1716100000:0;git status\npwd\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].command, "ls");
        assert!(entries[0].timestamp.is_none());
        assert_eq!(entries[1].command, "git status");
        assert_eq!(entries[1].timestamp, Some(1716100000));
        assert_eq!(entries[2].command, "pwd");
    }

    #[test]
    fn test_parse_empty_lines_skipped() {
        let content = "ls\n\n\ngit status\n\n";
        let entries = parse_zsh_history(content);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_import_to_empty_db() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("history.db");
        let hist_path = dir.path().join(".zsh_history");
        std::fs::write(
            &hist_path,
            ": 1716100000:0;ls -la\n: 1716100001:0;git status\n: 1716100002:0;cargo build\n",
        )
        .unwrap();

        let result = import_zsh_history(&db_path, &hist_path).unwrap();
        assert_eq!(result.total_lines, 3);
        assert_eq!(result.imported, 3);
        assert_eq!(result.skipped_sensitive, 0);
        assert_eq!(result.skipped_empty, 0);

        let conn = Connection::open(&db_path).unwrap();
        let count: u64 = conn
            .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_import_filters_sensitive() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("history.db");
        let hist_path = dir.path().join(".zsh_history");
        std::fs::write(
            &hist_path,
            "cargo build\ndocker login registry.io -u user -p secret123\nexport API_KEY=sk-abcdefghijklmnopqrstuvwxyz12345\ngit status\n",
        )
        .unwrap();

        let result = import_zsh_history(&db_path, &hist_path).unwrap();
        assert_eq!(result.imported, 2);
        assert_eq!(result.skipped_sensitive, 2);
    }

    #[test]
    fn test_import_deduplication() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("history.db");
        let hist_path = dir.path().join(".zsh_history");
        std::fs::write(
            &hist_path,
            ": 1716100000:0;ls -la\n: 1716200000:0;ls -la\n: 1716300000:0;ls -la\n",
        )
        .unwrap();

        let result = import_zsh_history(&db_path, &hist_path).unwrap();
        assert_eq!(result.imported, 3);

        let conn = Connection::open(&db_path).unwrap();
        let (count, ts): (i64, i64) = conn
            .query_row(
                "SELECT count, timestamp FROM commands WHERE command = 'ls -la'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 3);
        assert_eq!(ts, 1716300000);
    }

    #[test]
    fn test_import_preserves_timestamp() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("history.db");
        let hist_path = dir.path().join(".zsh_history");
        std::fs::write(&hist_path, ": 1716100000:0;git push\n").unwrap();

        import_zsh_history(&db_path, &hist_path).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let ts: i64 = conn
            .query_row(
                "SELECT timestamp FROM commands WHERE command = 'git push'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ts, 1716100000);
    }
}
