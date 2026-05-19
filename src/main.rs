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
        Commands::History { action } => match action {
            HistoryAction::Import { file, force } => cmd_history_import(file, force),
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
