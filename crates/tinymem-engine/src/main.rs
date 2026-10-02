use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use tinymem_engine::{
    CommandEmbedder, ContextHit, Engine, MemoryScope, RefreshReport, SearchOptions, mcp,
    snapshot_database,
};

#[derive(Parser)]
#[command(
    name = "tinymem",
    version,
    about = "Local agent memory and repository context"
)]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Make a consistent, unencrypted SQLite backup (explicit --db required).
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    /// Restore a validated backup to a NEW database path (explicit --db required).
    Restore {
        #[arg(long)]
        input: PathBuf,
    },
    /// Serve scoped personal memory and repository context over stdio MCP.
    Mcp {
        root: PathBuf,
        #[arg(long)]
        user: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        project: Option<String>,
        /// Trusted executable receiving JSON text and returning an embedding.
        #[arg(long, requires_all = ["embedding_model", "embedding_dimensions"])]
        embedding_command: Option<PathBuf>,
        /// Stable identity (change it when model weights/preprocessing change).
        #[arg(long, requires = "embedding_command")]
        embedding_model: Option<String>,
        #[arg(long, requires = "embedding_command")]
        embedding_dimensions: Option<usize>,
        #[arg(long, default_value_t = 30000)]
        embedding_timeout_ms: u64,
    },
    /// Reconcile the repository index.
    Index { root: PathBuf },
    /// Refresh the repository and retrieve current source context.
    Search {
        root: PathBuf,
        query: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long, default_value_t = 16000)]
        max_bytes: usize,
    },
    /// Poll the repository and emit a JSON report after each successful refresh.
    Watch {
        root: PathBuf,
        #[arg(
            long,
            default_value_t = 2000,
            help = "Polling interval (minimum 100 ms)"
        )]
        interval_ms: u64,
        #[arg(long)]
        once: bool,
    },
}

#[derive(Serialize)]
struct SearchOutput {
    repository: String,
    refresh: RefreshReport,
    hits: Vec<ContextHit>,
}

fn emit_json(value: &impl Serialize) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn watch(engine: &mut Engine, root: &Path, interval_ms: u64, once: bool) -> Result<()> {
    if once {
        return emit_json(&engine.refresh(root)?);
    }
    let (shutdown, signals) = mpsc::sync_channel(1);
    ctrlc::set_handler(move || {
        let _ = shutdown.try_send(());
    })
    .context("cannot install Ctrl-C handler")?;
    emit_json(&engine.refresh(root)?)?;
    let interval = Duration::from_millis(interval_ms.max(100));
    loop {
        match signals.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => match engine.refresh(root) {
                Ok(report) => emit_json(&report)?,
                Err(error) => eprintln!("tinymem: refresh failed: {error:#}; retrying"),
            },
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Commands::Backup { output } => {
            let source = cli.db.as_deref().context("backup requires explicit --db")?;
            snapshot_database(source, output)?;
            return emit_json(&serde_json::json!({"backup": output}));
        }
        Commands::Restore { input } => {
            let destination = cli
                .db
                .as_deref()
                .context("restore requires explicit --db")?;
            snapshot_database(input, destination)?;
            return emit_json(&serde_json::json!({"restored": destination}));
        }
        _ => {}
    }
    let root = match &cli.command {
        Commands::Backup { .. } | Commands::Restore { .. } => unreachable!(),
        Commands::Mcp { root, .. }
        | Commands::Index { root }
        | Commands::Search { root, .. }
        | Commands::Watch { root, .. } => root,
    };
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot resolve repository root {}", root.display()))?;
    ensure!(root.is_dir(), "repository root must be a directory");
    ensure!(root.to_str().is_some(), "repository root must be UTF-8");
    let db = cli
        .db
        .unwrap_or_else(|| root.join(".tinymem/repository.db"));
    if let Some(parent) = db.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create database directory {}", parent.display()))?;
    }
    let mut engine =
        Engine::open(&db).with_context(|| format!("cannot open database {}", db.display()))?;
    match cli.command {
        Commands::Backup { .. } | Commands::Restore { .. } => unreachable!(),
        Commands::Mcp {
            user,
            agent,
            project,
            embedding_command,
            embedding_model,
            embedding_dimensions,
            embedding_timeout_ms,
            ..
        } => {
            if let Some(command) = embedding_command {
                engine = engine.with_embedder(CommandEmbedder::new(
                    command,
                    embedding_model.context("embedding model is required")?,
                    embedding_dimensions.context("embedding dimensions are required")?,
                    Duration::from_millis(embedding_timeout_ms),
                )?)?;
            }
            mcp::serve(
                engine,
                root,
                MemoryScope {
                    user_id: user,
                    agent_id: agent,
                    project_id: project,
                },
            )
        }
        Commands::Index { .. } => emit_json(&engine.refresh(&root)?),
        Commands::Search {
            query,
            limit,
            max_bytes,
            ..
        } => {
            let refresh = engine.refresh(&root)?;
            let hits = engine.search(&root, &query, SearchOptions { limit, max_bytes })?;
            emit_json(&SearchOutput {
                repository: refresh.repository.clone(),
                refresh,
                hits,
            })
        }
        Commands::Watch {
            interval_ms, once, ..
        } => watch(&mut engine, &root, interval_ms, once),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tinymem: {error:#}");
            ExitCode::FAILURE
        }
    }
}
