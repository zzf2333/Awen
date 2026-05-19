use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct CommandRecord {
    pub command: String,
    pub exit_code: i32,
    pub stderr: Option<String>,
    pub cwd: String,
    pub timestamp: i64,
    pub duration_ms: Option<u64>,
}

pub struct SessionContext {
    commands: VecDeque<CommandRecord>,
    max_size: usize,
    pub current_cwd: String,
}

impl SessionContext {
    pub fn new(max_size: usize) -> Self {
        Self {
            commands: VecDeque::with_capacity(max_size),
            max_size,
            current_cwd: String::new(),
        }
    }

    pub fn record(&mut self, record: CommandRecord) {
        self.current_cwd = record.cwd.clone();
        if self.commands.len() >= self.max_size {
            self.commands.pop_front();
        }
        self.commands.push_back(record);
    }

    pub fn last_command(&self) -> Option<&CommandRecord> {
        self.commands.back()
    }

    pub fn last_exit_code(&self) -> Option<i32> {
        self.commands.back().map(|c| c.exit_code)
    }

    pub fn last_stderr(&self) -> Option<&str> {
        self.commands.back().and_then(|c| c.stderr.as_deref())
    }

    pub fn recent_commands(&self, n: usize) -> Vec<&str> {
        self.commands
            .iter()
            .rev()
            .take(n)
            .map(|c| c.command.as_str())
            .collect()
    }

    pub fn recent_command_strings(&self) -> Vec<String> {
        self.commands.iter().map(|c| c.command.clone()).collect()
    }

    pub fn is_failure_mode(&self) -> bool {
        self.last_exit_code().is_some_and(|c| c != 0)
    }

    pub fn consecutive_tool_count(&self, tool: &str) -> usize {
        self.commands
            .iter()
            .rev()
            .take_while(|c| {
                c.command
                    .split_whitespace()
                    .next()
                    .is_some_and(|cmd| cmd == tool)
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(cmd: &str, exit_code: i32) -> CommandRecord {
        CommandRecord {
            command: cmd.into(),
            exit_code,
            stderr: None,
            cwd: "/tmp".into(),
            timestamp: 0,
            duration_ms: None,
        }
    }

    #[test]
    fn test_record_and_retrieve() {
        let mut ctx = SessionContext::new(5);
        ctx.record(make_record("ls", 0));
        ctx.record(make_record("pwd", 0));
        assert_eq!(ctx.last_command().unwrap().command, "pwd");
        assert_eq!(ctx.recent_commands(3), vec!["pwd", "ls"]);
    }

    #[test]
    fn test_max_size() {
        let mut ctx = SessionContext::new(2);
        ctx.record(make_record("a", 0));
        ctx.record(make_record("b", 0));
        ctx.record(make_record("c", 0));
        assert_eq!(ctx.recent_commands(5), vec!["c", "b"]);
    }

    #[test]
    fn test_failure_mode() {
        let mut ctx = SessionContext::new(5);
        ctx.record(make_record("cargo build", 1));
        assert!(ctx.is_failure_mode());
        ctx.record(make_record("cargo add tokio", 0));
        assert!(!ctx.is_failure_mode());
    }

    #[test]
    fn test_consecutive_tool_count() {
        let mut ctx = SessionContext::new(10);
        ctx.record(make_record("npm install", 0));
        ctx.record(make_record("docker build .", 0));
        ctx.record(make_record("docker run app", 0));
        ctx.record(make_record("docker ps", 0));
        assert_eq!(ctx.consecutive_tool_count("docker"), 3);
    }
}
