use awen::config;
use awen::daemon;
use awen::layer::history::HistoryLayer;
use awen::layer::history_import;
use awen::protocol;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "awen",
    version,
    about = "Terminal Intelligence Layer — Smart when you need it. Silent when you don't."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Awen daemon
    Start,
    /// Stop the Awen daemon
    Stop,
    /// Show daemon status
    Status,
    /// Show daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short, long, default_value_t = 50)]
        lines: usize,
    },
    /// Open or show configuration
    Config,
    /// Show current context engine state
    Context,
    /// Configure shell integration (add source line to ~/.zshrc)
    Setup,
    /// Manage command history
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// Import commands from zsh history file
    Import {
        /// Path to history file (default: $HISTFILE or ~/.zsh_history)
        #[arg(long)]
        file: Option<PathBuf>,
        /// Force import even if database is not empty
        #[arg(long)]
        force: bool,
    },
    /// Show history database statistics
    Stats,
    /// Clear all history entries
    Clear {
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => cmd_start().await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Logs { lines } => cmd_logs(lines),
        Commands::Config => cmd_config(),
        Commands::Context => cmd_context().await,
        Commands::Setup => cmd_setup().await,
        Commands::History { action } => match action {
            HistoryAction::Import { file, force } => cmd_history_import(file, force),
            HistoryAction::Stats => cmd_history_stats(),
            HistoryAction::Clear { yes } => cmd_history_clear(yes),
        },
    }
}

async fn cmd_start() {
    if daemon::is_running() {
        eprintln!("awen daemon is already running");
        std::process::exit(1);
    }

    let config = config::load_config();

    std::fs::create_dir_all(config::data_dir()).ok();
    std::fs::create_dir_all(config::config_dir()).ok();

    let log_path = config::log_path();
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("failed to open log file");

    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("awen daemon starting");
    println!("awen daemon starting...");

    daemon::run(config).await;
}

async fn cmd_stop() {
    match daemon::send_shutdown().await {
        Ok(()) => println!("awen daemon stopped"),
        Err(e) => {
            eprintln!("failed to stop daemon: {e}");
            if let Some(pid) = daemon::read_pid() {
                eprintln!("trying to kill pid {pid}");
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                daemon::cleanup_socket();
                println!("awen daemon stopped (via signal)");
            } else {
                std::process::exit(1);
            }
        }
    }
}

async fn cmd_status() {
    if !daemon::is_running() {
        println!("awen daemon is not running");
        return;
    }

    match daemon::send_status_request().await {
        Ok(resp) => match resp {
            protocol::Response::Status(s) => {
                println!("awen daemon is running (pid: {})", s.pid);
                println!("  uptime: {}s", s.uptime_secs);
                println!("  history entries: {}", s.history_count);
                println!("  AI enabled: {}", s.ai_enabled);
            }
            _ => println!("awen daemon is running"),
        },
        Err(e) => {
            eprintln!("failed to query status: {e}");
        }
    }
}

fn cmd_logs(lines: usize) {
    let log_path = config::log_path();
    if !log_path.exists() {
        println!("no log file found at {}", log_path.display());
        return;
    }
    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let all_lines: Vec<&str> = content.lines().collect();
            let start = all_lines.len().saturating_sub(lines);
            for line in &all_lines[start..] {
                println!("{line}");
            }
        }
        Err(e) => eprintln!("failed to read logs: {e}"),
    }
}

fn cmd_config() {
    let config_path = config::config_dir().join("config.toml");
    if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => print!("{content}"),
            Err(e) => eprintln!("failed to read config: {e}"),
        }
    } else {
        let default = config::AwenConfig::default();
        let content = toml::to_string_pretty(&default).unwrap();
        println!("# No config file found. Default config:");
        println!("# Create at: {}", config_path.display());
        println!();
        print!("{content}");
    }
}

fn cmd_history_import(file: Option<PathBuf>, force: bool) {
    let db_path = config::history_db_path();

    if db_path.exists()
        && !force
        && let Ok(history) = HistoryLayer::new(&db_path)
    {
        let count = history.count();
        if count > 0 {
            eprintln!(
                "history database already has {count} entries. Use --force to import anyway."
            );
            std::process::exit(1);
        }
    }

    let histfile = file.unwrap_or_else(config::default_zsh_histfile);
    if !histfile.exists() {
        eprintln!("history file not found: {}", histfile.display());
        std::process::exit(1);
    }

    std::fs::create_dir_all(config::data_dir()).ok();
    println!("importing from {}...", histfile.display());

    match history_import::import_zsh_history(&db_path, &histfile) {
        Ok(r) => {
            println!("import complete:");
            println!("  total entries parsed: {}", r.total_lines);
            println!("  imported:             {}", r.imported);
            println!("  skipped (sensitive):  {}", r.skipped_sensitive);
            println!("  skipped (empty):      {}", r.skipped_empty);
        }
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_history_stats() {
    let db_path = config::history_db_path();
    if !db_path.exists() {
        println!("no history database found");
        return;
    }
    match HistoryLayer::new(&db_path) {
        Ok(history) => {
            let count = history.count();
            let size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
            println!("history database: {}", db_path.display());
            println!("  entries: {count}");
            println!("  size:    {:.1} KB", size as f64 / 1024.0);
        }
        Err(e) => eprintln!("failed to open history: {e}"),
    }
}

fn cmd_history_clear(yes: bool) {
    let db_path = config::history_db_path();
    if !db_path.exists() {
        println!("no history database found");
        return;
    }

    if !yes {
        eprint!("this will delete all history entries. continue? [y/N] ");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err()
            || !input.trim().eq_ignore_ascii_case("y")
        {
            println!("aborted");
            return;
        }
    }

    match std::fs::remove_file(&db_path) {
        Ok(()) => println!("history cleared"),
        Err(e) => eprintln!("failed to clear history: {e}"),
    }
}

async fn cmd_setup() {
    let config_dir = config::config_dir();
    let data_dir = config::data_dir();
    std::fs::create_dir_all(&config_dir).ok();
    std::fs::create_dir_all(&data_dir).ok();

    // 1. Generate default config if missing
    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default = config::AwenConfig::default();
        let content = toml::to_string_pretty(&default).unwrap();
        match std::fs::write(&config_path, &content) {
            Ok(()) => println!("[1/4] config: created {}", config_path.display()),
            Err(e) => eprintln!(
                "[1/4] config: failed to write {}: {e}",
                config_path.display()
            ),
        }
    } else {
        println!("[1/4] config: {}", config_path.display());
    }

    // 2. Find and configure plugin
    let plugin_path = find_plugin().unwrap_or_else(|| {
        eprintln!("error: could not find awen.zsh plugin");
        eprintln!("  checked: <brew_prefix>/share/awen/awen.zsh");
        eprintln!("  checked: {}/awen.zsh", config_dir.display());
        std::process::exit(1);
    });

    let home = dirs::home_dir().unwrap_or_else(|| {
        eprintln!("error: could not determine home directory");
        std::process::exit(1);
    });
    let zshrc = home.join(".zshrc");

    if zshrc.exists()
        && let Ok(content) = std::fs::read_to_string(&zshrc)
        && content.contains("awen.zsh")
    {
        println!("[2/4] shell: already configured in {}", zshrc.display());
    } else {
        let source_line = format!(
            "\n# Awen — Terminal Intelligence Layer\nsource {}\n",
            plugin_path.display()
        );
        if let Err(e) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&zshrc)
            .and_then(|mut f| std::io::Write::write_all(&mut f, source_line.as_bytes()))
        {
            eprintln!("error: failed to write to {}: {e}", zshrc.display());
            std::process::exit(1);
        }
        println!("[2/4] shell: added source line to {}", zshrc.display());
    }

    // 3. Restart daemon (stop old, start new)
    if daemon::is_running() {
        daemon::send_shutdown().await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if daemon::is_running() {
            if let Some(pid) = daemon::read_pid() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                daemon::cleanup_socket();
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| "awen".into());
    match std::process::Command::new(&exe).arg("start").spawn() {
        Ok(_) => {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            println!("[3/4] daemon: started");
        }
        Err(e) => {
            eprintln!("[3/4] daemon: failed to start: {e}");
            std::process::exit(1);
        }
    }

    // 4. Verify
    match daemon::send_status_request().await {
        Ok(protocol::Response::Status(s)) => {
            println!(
                "[4/4] status: pid {}, {} history entries",
                s.pid, s.history_count
            );
        }
        _ => {
            println!("[4/4] status: daemon running (could not query details)");
        }
    }

    println!();
    println!("awen setup complete! restart your shell or run: source ~/.zshrc");
}

fn find_plugin() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(|p| p.parent())
    {
        let brew_plugin = prefix.join("share/awen/awen.zsh");
        if brew_plugin.exists() {
            return Some(brew_plugin);
        }
    }
    let local = config::config_dir().join("awen.zsh");
    if local.exists() {
        return Some(local);
    }
    None
}

async fn cmd_context() {
    if !daemon::is_running() {
        println!("awen daemon is not running");
        return;
    }

    match daemon::send_context_request().await {
        Ok(resp) => match resp {
            protocol::Response::Context(c) => {
                println!("cwd: {}", c.cwd);
                if let Some(repo) = &c.repo_type {
                    println!("repo type: {repo}");
                }
                if let Some(branch) = &c.git_branch {
                    println!("git branch: {branch}");
                }
                if !c.recent_commands.is_empty() {
                    println!("recent commands:");
                    for cmd in &c.recent_commands {
                        println!("  {cmd}");
                    }
                }
                if let Some(code) = c.last_exit_code {
                    println!("last exit code: {code}");
                }
            }
            _ => println!("unexpected response"),
        },
        Err(e) => eprintln!("failed to query context: {e}"),
    }
}
