use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use cruxx_agentic::register_all as register_agentic;
use cruxx_script::{HandlerRegistry, Runner};
use cruxx_types::step::StepStatus;

use hooklings::config;
use hooklings::emit::{CheckResult, Emitter, Status};
use hooklings::handlers;

#[derive(Parser)]
#[command(name = "hooklings", about = "YAML-driven developer preflight checks")]
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
    /// Print the merged effective config
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::load();

    match cli.command {
        Command::Preflight { emit, pipeline } => {
            let mut registry = HandlerRegistry::new();
            register_agentic(&mut registry);
            handlers::register_all(&mut registry, &cfg);

            let pipeline_path = pipeline.unwrap_or_else(|| PathBuf::from(&cfg.pipeline.default));

            let pipeline_yaml = std::fs::read_to_string(&pipeline_path).map_err(|e| {
                anyhow::anyhow!("cannot read pipeline {}: {e}", pipeline_path.display())
            })?;

            let pipeline_def = cruxx_script::load(&pipeline_yaml)
                .map_err(|e| anyhow::anyhow!("pipeline parse error: {e}"))?;

            let runner = Runner::new(Arc::new(registry));
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
                    print!("{}", Emitter::markdown_table(&results));
                }
                EmitMode::Json => {}
            }
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
                .map_err(|e| anyhow::anyhow!("handler error: {e}"))?;
            println!("{}", serde_json::to_string_pretty(&result.value)?);
        }

        Command::Config {
            action: ConfigAction::Show,
        } => {
            let toml_str = toml::to_string_pretty(&cfg)?;
            println!("{toml_str}");
        }
    }

    Ok(())
}
