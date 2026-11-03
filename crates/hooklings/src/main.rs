//! Command-line interface for running hooklings checks and inspecting configuration.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use crux_agentic::register_all as register_agentic;
use crux_script::{HandlerRegistry, Runner};
use crux_types::step::StepStatus;

use hooklings::checkpoint::{self, CommitType, Outcome};
use hooklings::config;
use hooklings::emit::{self, CheckResult, Emitter, Status};
use hooklings::handlers;
use hooklings::push_range;

#[derive(Parser)]
#[command(
    name = "hooklings",
    version,
    about = "YAML-driven developer preflight checks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run all enabled checks in the configured pipeline
    Preflight {
        #[arg(long, value_enum, default_value = "both")]
        emit: EmitMode,
        /// Override pipeline file
        #[arg(long)]
        pipeline: Option<PathBuf>,
    },
    /// Run a single named check
    Check { name: String },
    /// Record paths an agent harness has touched since the last checkpoint
    Record {
        /// Paths to record, repository-relative or absolute
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Read paths from a hook payload on stdin instead of arguments
        #[arg(long)]
        stdin: bool,
        /// Report what would be recorded without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Commit everything recorded since the last checkpoint
    Checkpoint {
        /// Report the commit that would be made without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Replace the inferred subject line
        #[arg(long, value_name = "TEXT")]
        message: Option<String>,
        /// Replace the inferred conventional-commit type
        #[arg(long = "type", value_enum)]
        commit_type: Option<CommitTypeArg>,
        /// Replace the inferred conventional-commit scope
        #[arg(long)]
        scope: Option<String>,
        /// Exit non-zero when the checkpoint is refused
        #[arg(long)]
        strict: bool,
        /// Repository directory, defaults to the working directory
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Show the checkpoint state for a repository
    Status {
        /// Repository directory, defaults to the working directory
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Print the merged effective config
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Resolve the commit range(s) being pushed, read from git's pre-push stdin
    ResolvePushRange {
        /// Branch to compute the merge-base against for newly pushed branches
        #[arg(long, default_value = "main")]
        main_branch: String,
        /// Repository directory, defaults to the working directory
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Manage checkpoint coalesced commits
    Coalesce {
        /// Enable or disable coalesced commits
        #[arg(long, value_enum, default_value = "auto")]
        mode: CheckpointMode,
        /// Repository directory, defaults to the working directory
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Manage handoff pending default limits
    Handoff {
        /// Set a new default limit for handoff pending items
        #[arg(long)]
        set: Option<usize>,
        /// Show the current default limit and config
        #[arg(long, default_value = "false")]
        show: bool,
        /// Repository directory, defaults to the working directory
        #[arg(long)]
        repo: Option<PathBuf>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum CommitTypeArg {
    Feat,
    Fix,
    Docs,
    Test,
    Refactor,
    Style,
    Build,
    Chore,
}

/// Mode for checkpoint coalesced commits
#[derive(ValueEnum, Clone, Copy)]
enum CheckpointMode {
    Auto,
    On,
    Off,
}

impl From<CommitTypeArg> for CommitType {
    fn from(value: CommitTypeArg) -> Self {
        match value {
            CommitTypeArg::Feat => CommitType::Feat,
            CommitTypeArg::Fix => CommitType::Fix,
            CommitTypeArg::Docs => CommitType::Docs,
            CommitTypeArg::Test => CommitType::Test,
            CommitTypeArg::Refactor => CommitType::Refactor,
            CommitTypeArg::Style => CommitType::Style,
            CommitTypeArg::Build => CommitType::Build,
            CommitTypeArg::Chore => CommitType::Chore,
        }
    }
}

/// Extracts touched paths from a harness hook payload read from stdin.
fn paths_from_stdin() -> anyhow::Result<Vec<PathBuf>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_str(&raw)?;
    let candidates = match &value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::String(single) => vec![serde_json::Value::String(single.clone())],
        serde_json::Value::Object(map) => {
            let tool_input = map.get("tool_input").unwrap_or(&value);
            let found = ["file_path", "path", "notebook_path", "paths"]
                .iter()
                .find_map(|key| tool_input.get(*key).cloned());
            match found {
                Some(serde_json::Value::Array(items)) => items,
                Some(single) => vec![single],
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    };

    Ok(candidates
        .into_iter()
        .filter_map(|item| item.as_str().map(PathBuf::from))
        .collect())
}

/// Resolves harness-supplied paths to repository-relative form.
fn to_repo_relative(repo: &gix::Repository, path: &PathBuf) -> anyhow::Result<String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("cannot checkpoint a bare repository"))?;

    let candidate = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()?.join(path)
    };

    let normalized = candidate
        .canonicalize()
        .or_else(|_| {
            workdir
                .join(path.strip_prefix("./").unwrap_or(path))
                .canonicalize()
        })
        .unwrap_or(candidate);

    normalized
        .strip_prefix(workdir)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| anyhow::anyhow!("{} is outside the repository workdir", path.display()))
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[derive(Subcommand)]
enum ConfigAction {
    Show,
}

#[derive(ValueEnum, Clone)]
enum EmitMode {
    Json,
    Table,
    Both,
}

async fn run_preflight(
    cfg: &config::Config,
    emit: EmitMode,
    pipeline: Option<PathBuf>,
) -> anyhow::Result<()> {
    let mut registry = HandlerRegistry::new();
    register_agentic(&mut registry);
    handlers::register_all(&mut registry, cfg);

    let pipeline_path = pipeline.unwrap_or_else(|| PathBuf::from(&cfg.pipeline.default));
    let pipeline_yaml = std::fs::read_to_string(&pipeline_path)
        .map_err(|e| anyhow::anyhow!("cannot read pipeline {}: {e}", pipeline_path.display()))?;
    let pipeline_def = crux_script::load(&pipeline_yaml)
        .map_err(|e| anyhow::anyhow!("pipeline parse error: {e}"))?;

    let runner = Runner::new(Arc::new(registry));
    // Steps are marked `allow_failure: true` in the pipeline, so a failing check
    // becomes Status::Error for that step and the rest of the briefing still runs.
    // crux's default is to abort, which previously truncated the report at the first
    // error — the dropped checks were exactly the actionable ones (auth, reachability,
    // pending work). See the comment in ~/.config/hooklings/default.crux.
    let trace = runner.run(&pipeline_def, serde_json::json!({})).await;

    let results: Vec<CheckResult> = trace
        .steps
        .iter()
        .filter(|s| s.status != StepStatus::Rejected)
        .map(|step| {
            let output = step.output.clone().unwrap_or(serde_json::Value::Null);
            let status = if step.status == StepStatus::Ok {
                match output.get("status").and_then(|s| s.as_str()) {
                    Some("warn") => Status::Warn,
                    Some("skip") => Status::Skip,
                    Some("fail") => Status::Fail,
                    _ => Status::Pass,
                }
            } else {
                Status::Error
            };
            CheckResult {
                name: step.name.clone(),
                status,
                detail: output
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
                data: Some(output),
            }
        })
        .collect();

    let emitter = Emitter::new("preflight".into());
    match emit {
        EmitMode::Json | EmitMode::Both => {
            let json_path = PathBuf::from(&cfg.emit.json_path);
            emitter.write_json(&results, &json_path)?;
        }
        EmitMode::Table => {}
    }
    match emit {
        EmitMode::Table | EmitMode::Both => {
            print!("{}", emit::markdown_table(&results));
        }
        EmitMode::Json => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("completions") {
        clap_complete::generate(
            clap_complete_nushell::Nushell,
            &mut Cli::command(),
            "hooklings",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    let cli = Cli::parse();

    let cfg = config::Config::load();

    match cli.command {
        Command::Preflight { emit, pipeline } => {
            run_preflight(&cfg, emit, pipeline).await?;
        }

        Command::Check { name } => {
            let mut registry = HandlerRegistry::new();
            register_agentic(&mut registry);
            handlers::register_all(&mut registry, &cfg);

            let handler = registry
                .get_handler(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown handler: {name}"))?
                .clone();

            let result = handler(serde_json::json!({}))
                .await
                .outcome
                .map_err(|e| anyhow::anyhow!("handler error: {e}"))?;
            println!("{}", serde_json::to_string_pretty(&result.value)?);
        }

        Command::Record {
            paths,
            stdin,
            dry_run,
        } => {
            let cwd = std::env::current_dir()?;
            let repo = checkpoint::discover(&cwd)?;

            let raw = if stdin { paths_from_stdin()? } else { paths };

            let mut relative = Vec::new();
            for path in &raw {
                relative.push(to_repo_relative(&repo, path)?);
            }

            if dry_run {
                for path in &relative {
                    println!("would record {path}");
                }
                return Ok(());
            }

            let added = checkpoint::record_paths(&repo, &relative, &now_rfc3339())?;
            println!("recorded {} new path(s)", added.len());
            for path in &added {
                println!("  {path}");
            }
        }

        Command::Checkpoint {
            dry_run,
            message,
            commit_type,
            scope,
            strict,
            repo: repo_dir,
        } => {
            let cwd = repo_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let repo = checkpoint::discover(&cwd)?;

            let request = checkpoint::Request {
                config: &cfg.checkpoint,
                subject_override: message,
                type_override: commit_type.map(Into::into),
                scope_override: scope,
                dry_run,
            };

            let report = checkpoint::commit::run(&repo, &request)?;

            for rejected in &report.rejected {
                eprintln!("skipped {} ({})", rejected.path, rejected.reason);
            }

            let refused = matches!(report.outcome, Outcome::Refused(_));

            match &report.outcome {
                Outcome::Committed {
                    id,
                    subject,
                    files,
                    added,
                    removed,
                } => {
                    println!("committed {id} {subject}");
                    println!("  {files} file(s), +{added} -{removed}");
                }
                Outcome::Planned {
                    subject,
                    files,
                    added,
                    removed,
                } => {
                    println!("would commit {subject}");
                    println!("  {files} file(s), +{added} -{removed}");
                }
                Outcome::Skipped(reason) => println!("skipped: {reason}"),
                Outcome::Refused(reason) => println!("refused: {reason}"),
            }

            if refused && strict {
                std::process::exit(1);
            }
        }

        Command::Status { repo: repo_dir } => {
            let cwd = repo_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let repo = checkpoint::discover(&cwd)?;
            let record = checkpoint::load_record(&repo)?;

            println!(
                "branch:  {}",
                checkpoint::commit::current_branch(&repo).unwrap_or_else(|| "-".into())
            );
            println!("enabled: {}", cfg.checkpoint.enabled);
            println!(
                "record:  {} path(s), {} edit(s)",
                record.paths.len(),
                record.edits
            );
            for path in &record.paths {
                println!("  {path}");
            }
        }

        Command::Config {
            action: ConfigAction::Show,
        } => {
            let toml_str = toml::to_string_pretty(&cfg)?;
            println!("{toml_str}");
        }

        Command::Coalesce { mode, repo } => {
            let cwd = match repo {
                Some(repo) => repo,
                None => std::env::current_dir().unwrap_or_default(),
            };
            let cfg_path = cwd.join(".hooklings.toml");
            let cfg_str = std::fs::read_to_string(&cfg_path).unwrap_or_else(|_| {
                String::from(
                    r#"[checks]
handoff_pending = { db = "", default_limit = 20 }

[checkpoint]
enabled = false
max_files = 40
max_lines = 2000
allow_protected_branch = false
protected_branches = ["develop", "release"]

[pipeline]
default = "{HOME}/.config/hooklings/default.crux

[emit]
json_path = ".ctx/last-preflight.json""#,
                )
            });
            let mut cfg: config::Config = match toml::from_str(&cfg_str) {
                Ok(c) => c,
                _err => {
                    println!("could not parse {:?}, showing auto mode only", cfg_path);
                    match mode {
                        CheckpointMode::Auto => {
                            println!("checkpoint coalescing mode: auto");
                        }
                        CheckpointMode::On => {
                            println!("checkpoint coalescing mode: on");
                        }
                        CheckpointMode::Off => {
                            println!("checkpoint coalescing mode: off");
                        }
                    }
                    return Ok(());
                }
            };

            match mode {
                CheckpointMode::Auto => {
                    let on = cfg.checkpoint.enabled
                        && checkpoint::commit::current_branch(&checkpoint::discover(&cwd)?)
                            .map(|b| !b.is_empty())
                            .unwrap_or(false);
                    if on {
                        println!(
                            "checkpoint coalescing mode: auto (enabled on non-protected branches)"
                        );
                    } else {
                        println!(
                            "checkpoint coalescing mode: auto (disabled on protected branch or not enabled)"
                        );
                    }
                }
                CheckpointMode::On => {
                    cfg.checkpoint.enabled = true;
                    let new_toml = toml::to_string_pretty(&cfg).unwrap();
                    let _ = std::fs::write(&cfg_path, &new_toml);
                    println!("checkpoint coalescing mode: on (enabled)");
                }
                CheckpointMode::Off => {
                    cfg.checkpoint.enabled = false;
                    let new_toml = toml::to_string_pretty(&cfg).unwrap();
                    let _ = std::fs::write(&cfg_path, &new_toml);
                    println!("checkpoint coalescing mode: off (disabled)");
                }
            }
        }

        Command::Handoff { set, show, repo } => {
            let cwd = match repo {
                Some(repo) => repo,
                None => std::env::current_dir().unwrap_or_default(),
            };
            let cfg_path = cwd.join(".hooklings.toml");
            let cfg_str = std::fs::read_to_string(&cfg_path).unwrap_or_else(|_| {
                String::from(
                    r#"[checks]
handoff_pending = { db = "", default_limit = 20 }

[checkpoint]
enabled = false
max_files = 40
max_lines = 2000
allow_protected_branch = false
protected_branches = ["develop", "release"]

[pipeline]
default = "{HOME}/.config/hooklings/default.crux

[emit]
json_path = ".ctx/last-preflight.json""#,
                )
            });
            let mut cfg: config::Config = match toml::from_str(&cfg_str) {
                Ok(c) => c,
                _err => {
                    println!(
                        "could not parse {:?}, showing handoff config only",
                        cfg_path
                    );
                    if show {
                        println!("handoff_pending.default_limit = 20");
                        println!("handoff_pending.db = ");
                    }
                    return Ok(());
                }
            };

            if show {
                let hc = &cfg.checks.handoff_pending;
                println!("handoff_pending.default_limit = {}", hc.default_limit);
                println!("handoff_pending.db = {}", hc.db);
            }

            if let Some(n) = set {
                cfg.checks.handoff_pending.default_limit = n;
                let new_toml = toml::to_string_pretty(&cfg).unwrap();
                let _ = std::fs::write(&cfg_path, &new_toml);
                println!("handoff_pending.default_limit set to {}", n);
            }
        }

        Command::ResolvePushRange { main_branch, repo } => {
            let cwd = match repo {
                Some(repo) => repo,
                None => std::env::current_dir()?,
            };
            let repo = checkpoint::discover(&cwd)?;

            let stdin = std::io::stdin();
            let ranges = push_range::resolve(&repo, stdin.lock(), &main_branch)?;
            for range in &ranges {
                println!("{range}");
            }
        }
    }

    Ok(())
}
