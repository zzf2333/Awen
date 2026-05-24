use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::config::FilesystemConfig;
use crate::protocol::{RequestContext, Suggestion, SuggestionSource};
use crate::sanitize::is_sensitive_path;

const DIR_ONLY_COMMANDS: &[&str] = &["cd", "pushd", "popd", "z", "zoxide"];

const FILE_ONLY_COMMANDS: &[&str] = &[
    "cat", "bat", "vim", "vi", "nvim", "nano", "code", "less", "more", "head", "tail", "wc",
    "sort", "uniq", "diff", "patch", "source",
];

const FILE_COMMANDS: &[&str] = &[
    "cd", "pushd", "popd", "z", "zoxide", "cat", "bat", "vim", "vi", "nvim", "nano", "code",
    "less", "more", "head", "tail", "wc", "sort", "uniq", "diff", "patch", "source", "rm", "cp",
    "mv", "mkdir", "touch", "ln", "chmod", "chown", "open", "xdg-open",
];

const HIDDEN_ENTRIES: &[&str] = &[".git", "target", "dist", "__pycache__", ".DS_Store", ".hg"];

const MONOREPO_DIRS: &[&str] = &[
    "packages", "apps", "services", "crates", "libs", "modules", "src",
];

const MAX_CACHE_SLOTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryFilter {
    DirectoriesOnly,
    FilesOnly,
    All,
}

#[derive(Debug)]
struct PathContext {
    fragment: String,
    scan_dir: PathBuf,
    match_prefix: String,
    filter: EntryFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryVisibility {
    Normal,
    Hidden,
    Deprioritized,
    Sensitive,
}

#[derive(Debug, Clone)]
struct DirEntry {
    name: String,
    is_dir: bool,
    modified: Option<SystemTime>,
    visibility: EntryVisibility,
}

struct CachedDir {
    entries: Vec<DirEntry>,
    scanned_at: Instant,
}

pub struct FilesystemLayer {
    cache: HashMap<PathBuf, CachedDir>,
    config: FilesystemConfig,
}

impl FilesystemLayer {
    pub fn new(config: FilesystemConfig) -> Self {
        Self {
            cache: HashMap::new(),
            config,
        }
    }

    pub fn suggest(
        &mut self,
        input: &str,
        _cursor_pos: usize,
        context: &RequestContext,
    ) -> Vec<Suggestion> {
        if !self.config.enabled {
            return Vec::new();
        }

        let Some(ctx) = parse_path_context(input, &context.cwd) else {
            return Vec::new();
        };

        if is_sensitive_path(ctx.scan_dir.to_string_lossy().as_ref()) {
            return Vec::new();
        }

        let entries = self.get_entries(&ctx.scan_dir);
        let filtered = filter_entries(entries, &ctx.filter);
        let scored = score_matches(&filtered, &ctx.match_prefix, &ctx.fragment, &context.cwd);

        build_suggestions(scored, &ctx, 8)
    }

    fn get_entries(&mut self, dir: &Path) -> &[DirEntry] {
        let ttl = std::time::Duration::from_millis(self.config.cache_ttl_ms);
        let need_scan = match self.cache.get(dir) {
            Some(cached) => cached.scanned_at.elapsed() > ttl,
            None => true,
        };

        if need_scan {
            let entries = scan_directory(dir, self.config.max_scan_entries);
            if self.cache.len() >= MAX_CACHE_SLOTS
                && let Some(oldest_key) = self
                    .cache
                    .iter()
                    .min_by_key(|(_, v)| v.scanned_at)
                    .map(|(k, _)| k.clone())
            {
                self.cache.remove(&oldest_key);
            }
            self.cache.insert(
                dir.to_path_buf(),
                CachedDir {
                    entries,
                    scanned_at: Instant::now(),
                },
            );
        }

        &self.cache.get(dir).unwrap().entries
    }
}

fn extract_last_shell_token(args: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in args.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        current
    } else if args.ends_with(' ') && !in_single && !in_double {
        String::new()
    } else {
        tokens.pop().unwrap_or_default()
    }
}

fn parse_path_context(input: &str, cwd: &str) -> Option<PathContext> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
    let command = parts[0];

    let args_part = if parts.len() > 1 { parts[1] } else { "" };

    let trailing_space = input.ends_with(' ') || (parts.len() > 1 && args_part.is_empty());

    let fragment = if parts.len() == 1 && !trailing_space {
        return None;
    } else if trailing_space && (parts.len() == 1 || args_part.trim_end().is_empty()) {
        String::new()
    } else {
        extract_last_shell_token(args_part)
    };

    let is_file_command = FILE_COMMANDS.contains(&command);
    let has_path_indicator =
        fragment.contains('/') || fragment.starts_with('.') || fragment.starts_with('~');

    if !is_file_command && !has_path_indicator && fragment.is_empty() {
        return None;
    }
    if !is_file_command && !has_path_indicator {
        return None;
    }

    let filter = if DIR_ONLY_COMMANDS.contains(&command) {
        EntryFilter::DirectoriesOnly
    } else if FILE_ONLY_COMMANDS.contains(&command) {
        EntryFilter::FilesOnly
    } else {
        EntryFilter::All
    };

    let (scan_dir, match_prefix) = resolve_path(&fragment, cwd);

    Some(PathContext {
        fragment,
        scan_dir,
        match_prefix,
        filter,
    })
}

fn resolve_path(fragment: &str, cwd: &str) -> (PathBuf, String) {
    if fragment.is_empty() {
        return (PathBuf::from(cwd), String::new());
    }

    let expanded = if let Some(rest) = fragment.strip_prefix("~/") {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        home.join(rest).to_string_lossy().into_owned()
    } else if fragment == "~" {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        home.to_string_lossy().into_owned()
    } else {
        fragment.to_string()
    };

    if let Some(pos) = expanded.rfind('/') {
        let dir_part = &expanded[..=pos];
        let prefix = &expanded[pos + 1..];

        let scan_dir = if Path::new(dir_part).is_absolute() {
            PathBuf::from(dir_part)
        } else {
            PathBuf::from(cwd).join(dir_part)
        };

        (scan_dir, prefix.to_string())
    } else {
        (PathBuf::from(cwd), expanded)
    }
}

fn scan_directory(dir: &Path, max_entries: usize) -> Vec<DirEntry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for entry in read_dir {
        if entries.len() >= max_entries {
            break;
        }
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };

        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let modified = meta.as_ref().and_then(|m| m.modified().ok());

        let full_path = dir.join(&name);
        let visibility = classify_visibility(&name, &full_path);

        entries.push(DirEntry {
            name,
            is_dir,
            modified,
            visibility,
        });
    }

    entries
}

fn classify_visibility(name: &str, full_path: &Path) -> EntryVisibility {
    if is_sensitive_path(full_path.to_string_lossy().as_ref()) {
        return EntryVisibility::Sensitive;
    }

    if HIDDEN_ENTRIES.contains(&name) {
        return EntryVisibility::Hidden;
    }

    if name == "node_modules" {
        return EntryVisibility::Deprioritized;
    }

    EntryVisibility::Normal
}

fn filter_entries<'a>(entries: &'a [DirEntry], filter: &EntryFilter) -> Vec<&'a DirEntry> {
    entries
        .iter()
        .filter(|e| {
            e.visibility != EntryVisibility::Hidden && e.visibility != EntryVisibility::Sensitive
        })
        .filter(|e| match filter {
            EntryFilter::DirectoriesOnly => e.is_dir,
            EntryFilter::FilesOnly => !e.is_dir,
            EntryFilter::All => true,
        })
        .collect()
}

fn score_matches<'a>(
    entries: &[&'a DirEntry],
    match_prefix: &str,
    _fragment: &str,
    cwd: &str,
) -> Vec<(&'a DirEntry, f64)> {
    if entries.is_empty() {
        return Vec::new();
    }

    if match_prefix.is_empty() {
        return entries
            .iter()
            .map(|e| {
                let mut score = 50.0;
                score *= recency_factor(e.modified);
                if e.visibility == EntryVisibility::Deprioritized {
                    score *= 0.3;
                }
                if e.is_dir && MONOREPO_DIRS.contains(&e.name.as_str()) {
                    score *= 1.2;
                }
                (*e, score)
            })
            .collect();
    }

    let mut results = Vec::new();

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Atom::new(
        match_prefix,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );

    let cwd_path = Path::new(cwd);

    for entry in entries {
        let mut haystack_buf = Vec::new();
        let haystack = Utf32Str::new(&entry.name, &mut haystack_buf);
        let Some(nucleo_score) = pattern.score(haystack, &mut matcher) else {
            continue;
        };

        let mut score = nucleo_score as f64;

        if entry
            .name
            .to_lowercase()
            .starts_with(&match_prefix.to_lowercase())
        {
            score += 100.0;
        }

        score *= recency_factor(entry.modified);

        if entry.visibility == EntryVisibility::Deprioritized {
            score *= 0.3;
        }

        if entry.is_dir && MONOREPO_DIRS.contains(&entry.name.as_str()) {
            score *= 1.2;
        }

        let _ = cwd_path;

        results.push((*entry, score));
    }

    results
}

fn recency_factor(modified: Option<SystemTime>) -> f64 {
    let Some(mtime) = modified else {
        return 1.0;
    };
    let Ok(elapsed) = mtime.elapsed() else {
        return 1.0;
    };
    let hours = elapsed.as_secs_f64() / 3600.0;
    if hours < 1.0 {
        1.5
    } else {
        1.0 / hours.ln().max(1.0)
    }
}

fn build_suggestions(
    mut scored: Vec<(&DirEntry, f64)>,
    ctx: &PathContext,
    limit: usize,
) -> Vec<Suggestion> {
    if scored.is_empty() {
        return Vec::new();
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let max_score = scored[0].1;
    let cutoff = max_score * 0.2;
    scored.retain(|(_, score)| *score >= cutoff);
    scored.truncate(limit);
    let base_dir = if ctx.fragment.is_empty() {
        String::new()
    } else if let Some(pos) = ctx.fragment.rfind('/') {
        ctx.fragment[..=pos].to_string()
    } else {
        String::new()
    };

    scored
        .into_iter()
        .map(|(entry, score)| {
            let suffix = if entry.is_dir { "/" } else { "" };
            let text = format!("{}{}{}", base_dir, entry.name, suffix);
            let confidence = (score / max_score).min(1.0) * 0.85;

            Suggestion {
                text,
                source: SuggestionSource::Filesystem,
                confidence,
                description: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context(cwd: &str) -> RequestContext {
        RequestContext {
            cwd: cwd.to_string(),
            last_command: None,
            last_exit_code: None,
            last_stderr: None,
            git_branch: None,
            git_status: None,
            session_commands: vec![],
            env_hints: vec![],
        }
    }

    #[test]
    fn test_parse_cd_directory() {
        let ctx = parse_path_context("cd src/", "/home/user").unwrap();
        assert_eq!(ctx.filter, EntryFilter::DirectoriesOnly);
        assert_eq!(ctx.scan_dir, PathBuf::from("/home/user/src/"));
        assert_eq!(ctx.match_prefix, "");
    }

    #[test]
    fn test_parse_cd_with_prefix() {
        let ctx = parse_path_context("cd sr", "/home/user").unwrap();
        assert_eq!(ctx.filter, EntryFilter::DirectoriesOnly);
        assert_eq!(ctx.scan_dir, PathBuf::from("/home/user"));
        assert_eq!(ctx.match_prefix, "sr");
    }

    #[test]
    fn test_parse_cat_file() {
        let ctx = parse_path_context("cat main.rs", "/home/user").unwrap();
        assert_eq!(ctx.filter, EntryFilter::FilesOnly);
        assert_eq!(ctx.match_prefix, "main.rs");
    }

    #[test]
    fn test_parse_no_path_context() {
        assert!(parse_path_context("git", "/tmp").is_none());
        assert!(parse_path_context("", "/tmp").is_none());
    }

    #[test]
    fn test_parse_non_file_command_no_path_indicator() {
        assert!(parse_path_context("git checkout", "/tmp").is_none());
    }

    #[test]
    fn test_parse_non_file_command_with_path_indicator() {
        let ctx = parse_path_context("git add ./src/", "/tmp").unwrap();
        assert_eq!(ctx.filter, EntryFilter::All);
        assert_eq!(ctx.scan_dir, PathBuf::from("/tmp/./src/"));
    }

    #[test]
    fn test_parse_tilde_expansion() {
        let ctx = parse_path_context("cd ~/Do", "/tmp").unwrap();
        assert_eq!(ctx.match_prefix, "Do");
        let home = dirs::home_dir().unwrap();
        assert_eq!(ctx.scan_dir, home.join(""));
    }

    #[test]
    fn test_parse_absolute_path() {
        let ctx = parse_path_context("cat /etc/hos", "/tmp").unwrap();
        assert_eq!(ctx.scan_dir, PathBuf::from("/etc/"));
        assert_eq!(ctx.match_prefix, "hos");
    }

    #[test]
    fn test_parse_cd_trailing_space() {
        let ctx = parse_path_context("cd ", "/home/user").unwrap();
        assert_eq!(ctx.filter, EntryFilter::DirectoriesOnly);
        assert_eq!(ctx.scan_dir, PathBuf::from("/home/user"));
        assert_eq!(ctx.match_prefix, "");
    }

    #[test]
    fn test_resolve_path_empty() {
        let (dir, prefix) = resolve_path("", "/home/user");
        assert_eq!(dir, PathBuf::from("/home/user"));
        assert_eq!(prefix, "");
    }

    #[test]
    fn test_resolve_path_relative() {
        let (dir, prefix) = resolve_path("src/main", "/home/user");
        assert_eq!(dir, PathBuf::from("/home/user/src/"));
        assert_eq!(prefix, "main");
    }

    #[test]
    fn test_resolve_path_absolute() {
        let (dir, prefix) = resolve_path("/usr/local/bin", "/tmp");
        assert_eq!(dir, PathBuf::from("/usr/local/"));
        assert_eq!(prefix, "bin");
    }

    #[test]
    fn test_classify_visibility_sensitive() {
        assert_eq!(
            classify_visibility(".env", Path::new("/home/user/.env")),
            EntryVisibility::Sensitive
        );
        assert_eq!(
            classify_visibility(".ssh", Path::new("/home/user/.ssh")),
            EntryVisibility::Sensitive
        );
    }

    #[test]
    fn test_classify_visibility_hidden() {
        assert_eq!(
            classify_visibility(".git", Path::new("/project/.git")),
            EntryVisibility::Hidden
        );
        assert_eq!(
            classify_visibility("target", Path::new("/project/target")),
            EntryVisibility::Hidden
        );
        assert_eq!(
            classify_visibility("dist", Path::new("/project/dist")),
            EntryVisibility::Hidden
        );
    }

    #[test]
    fn test_classify_visibility_deprioritized() {
        assert_eq!(
            classify_visibility("node_modules", Path::new("/project/node_modules")),
            EntryVisibility::Deprioritized
        );
    }

    #[test]
    fn test_classify_visibility_normal() {
        assert_eq!(
            classify_visibility("src", Path::new("/project/src")),
            EntryVisibility::Normal
        );
    }

    #[test]
    fn test_filter_entries_dirs_only() {
        let entries = vec![
            DirEntry {
                name: "src".into(),
                is_dir: true,
                modified: None,
                visibility: EntryVisibility::Normal,
            },
            DirEntry {
                name: "main.rs".into(),
                is_dir: false,
                modified: None,
                visibility: EntryVisibility::Normal,
            },
        ];
        let filtered = filter_entries(&entries, &EntryFilter::DirectoriesOnly);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "src");
    }

    #[test]
    fn test_filter_entries_files_only() {
        let entries = vec![
            DirEntry {
                name: "src".into(),
                is_dir: true,
                modified: None,
                visibility: EntryVisibility::Normal,
            },
            DirEntry {
                name: "main.rs".into(),
                is_dir: false,
                modified: None,
                visibility: EntryVisibility::Normal,
            },
        ];
        let filtered = filter_entries(&entries, &EntryFilter::FilesOnly);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "main.rs");
    }

    #[test]
    fn test_filter_entries_removes_hidden_and_sensitive() {
        let entries = vec![
            DirEntry {
                name: "src".into(),
                is_dir: true,
                modified: None,
                visibility: EntryVisibility::Normal,
            },
            DirEntry {
                name: ".git".into(),
                is_dir: true,
                modified: None,
                visibility: EntryVisibility::Hidden,
            },
            DirEntry {
                name: ".env".into(),
                is_dir: false,
                modified: None,
                visibility: EntryVisibility::Sensitive,
            },
        ];
        let filtered = filter_entries(&entries, &EntryFilter::All);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "src");
    }

    #[test]
    fn test_suggest_with_real_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=x").unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        let names: Vec<&str> = suggestions.iter().map(|s| s.text.as_str()).collect();
        assert!(names.contains(&"src/"));
        assert!(names.contains(&"tests/"));
        assert!(!names.iter().any(|n| n.contains(".git")));
        assert!(!names.iter().any(|n| n.contains(".env")));
    }

    #[test]
    fn test_suggest_cd_only_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        assert!(suggestions.iter().all(|s| s.text.ends_with('/')));
    }

    #[test]
    fn test_suggest_cat_only_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("lib.rs"), "").unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cat ", 4, &ctx);
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().all(|s| !s.text.ends_with('/')));
    }

    #[test]
    fn test_suggest_prefix_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("specs")).unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd s", 4, &ctx);
        let names: Vec<&str> = suggestions.iter().map(|s| s.text.as_str()).collect();
        assert!(names.contains(&"src/"));
        assert!(names.contains(&"specs/"));
        assert!(!names.contains(&"tests/"));
    }

    #[test]
    fn test_suggest_node_modules_depriority() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("node_modules")).unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        let src = suggestions.iter().find(|s| s.text == "src/");
        let nm = suggestions.iter().find(|s| s.text == "node_modules/");
        assert!(src.is_some());
        assert!(nm.is_some());
        assert!(src.unwrap().confidence > nm.unwrap().confidence);
    }

    #[test]
    fn test_suggest_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_disabled() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let mut config = FilesystemConfig::default();
        config.enabled = false;
        let mut layer = FilesystemLayer::new(config);
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_sensitive_dir_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let ssh_dir = dir.path().join(".ssh");
        std::fs::create_dir(&ssh_dir).unwrap();
        std::fs::write(ssh_dir.join("id_rsa"), "").unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let cwd = ssh_dir.to_str().unwrap();
        let ctx = make_context(cwd);

        let suggestions = layer.suggest("cat ", 4, &ctx);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_source_is_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd ", 3, &ctx);
        assert!(
            suggestions
                .iter()
                .all(|s| s.source == SuggestionSource::Filesystem)
        );
    }

    #[test]
    fn test_cache_reuse() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        layer.suggest("cd ", 3, &ctx);
        assert!(layer.cache.contains_key(dir.path()));

        std::fs::create_dir(dir.path().join("new_dir")).unwrap();
        let suggestions = layer.suggest("cd ", 3, &ctx);
        assert!(!suggestions.iter().any(|s| s.text == "new_dir/"));
    }

    #[test]
    fn test_suggest_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(src.join("layer")).unwrap();
        std::fs::create_dir(src.join("context")).unwrap();
        std::fs::write(src.join("main.rs"), "").unwrap();

        let mut layer = FilesystemLayer::new(FilesystemConfig::default());
        let ctx = make_context(dir.path().to_str().unwrap());

        let suggestions = layer.suggest("cd src/", 7, &ctx);
        let names: Vec<&str> = suggestions.iter().map(|s| s.text.as_str()).collect();
        assert!(names.contains(&"src/layer/"));
        assert!(names.contains(&"src/context/"));
    }

    #[test]
    fn test_recency_factor() {
        let recent = Some(SystemTime::now());
        let old = Some(SystemTime::now() - std::time::Duration::from_secs(86400));

        assert!(recency_factor(recent) > recency_factor(old));
        assert!((recency_factor(None) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_simple() {
        assert_eq!(extract_last_shell_token("arg1 arg2"), "arg2");
    }

    #[test]
    fn test_extract_trailing_space() {
        assert_eq!(extract_last_shell_token("arg1 arg2 "), "");
    }

    #[test]
    fn test_extract_double_quotes() {
        assert_eq!(extract_last_shell_token(r#""My Project/""#), "My Project/");
    }

    #[test]
    fn test_extract_unclosed_quote() {
        assert_eq!(extract_last_shell_token(r#""My Pro"#), "My Pro");
    }

    #[test]
    fn test_extract_single_quotes() {
        assert_eq!(extract_last_shell_token("'My Project/'"), "My Project/");
    }

    #[test]
    fn test_extract_backslash_escape() {
        assert_eq!(extract_last_shell_token(r"foo\ bar"), "foo bar");
    }

    #[test]
    fn test_extract_mixed_args() {
        assert_eq!(extract_last_shell_token(r#"file1.txt "My File""#), "My File");
    }

    #[test]
    fn test_parse_quoted_path() {
        let ctx = parse_path_context(r#"cd "My Project/""#, "/home/user").unwrap();
        assert_eq!(ctx.fragment, "My Project/");
    }

    #[test]
    fn test_parse_backslash_escaped_path() {
        let ctx = parse_path_context(r"cat foo\ bar", "/home/user").unwrap();
        assert_eq!(ctx.fragment, "foo bar");
    }
}
