//! `soroban-gauge`: resource-regression gate for Soroban contracts.
//!
//! Exit codes: 0 ok; 1 regression over threshold; 2 usage or config error;
//! 3 scenario execution failure; 4 baseline/environment mismatch; 5 I/O or parse error.
#![forbid(unsafe_code)]

mod cmds;

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use gauge_core::CoreError;

/// When to color output. Output currently contains no ANSI escapes at all, so
/// this is accepted for forward compatibility and has no effect yet.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ColorChoice {
    /// Follow the terminal and `NO_COLOR`.
    Auto,
    /// Always.
    Always,
    /// Never.
    Never,
}

/// Format for `inspect`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InspectFormat {
    /// Human-readable text.
    Table,
    /// JSON.
    Json,
}

/// Format for `diff` and `check`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DiffFormat {
    /// Aligned text.
    Table,
    /// PR comment body.
    Markdown,
    /// Structured JSON.
    Json,
}

/// Failure policy.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FailOnArg {
    /// Fail when a metric exceeds its threshold.
    Regression,
    /// Fail on any difference, including new and removed entries.
    AnyChange,
    /// Never fail.
    Never,
}

/// Flags shared by `diff` and `check`.
#[derive(Debug, clap::Args)]
pub struct CompareArgs {
    /// Output format (written to stdout).
    #[arg(long, value_enum, default_value_t = DiffFormat::Table)]
    pub format: DiffFormat,
    /// When to exit with code 1.
    #[arg(long, value_enum, default_value_t = FailOnArg::Regression)]
    pub fail_on: FailOnArg,
    /// Compare even when the SDK/host/protocol versions differ (prints a warning).
    #[arg(long)]
    pub allow_version_mismatch: bool,
    /// Also write the structured JSON diff to this file (whatever `--format` is).
    #[arg(long)]
    pub diff_out: Option<PathBuf>,
}

#[derive(Debug, Parser)]
#[command(
    name = "soroban-gauge",
    version,
    about = "Resource-regression gate for Soroban contracts"
)]
pub struct Cli {
    /// Config file.
    #[arg(long, global = true, default_value = "soroban-gauge.toml")]
    pub config: PathBuf,
    /// Print only errors.
    #[arg(long, global = true)]
    pub quiet: bool,
    /// More log detail on stderr (repeatable).
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// Color mode.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a starter config and an example scenario.
    Init {
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },
    /// Static report on a Wasm file (never executes it).
    Inspect {
        /// Path to the `.wasm`.
        wasm: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = InspectFormat::Table)]
        format: InspectFormat,
    },
    /// Execute all scenarios and write a run report. Does not touch the baseline.
    Run {
        /// Report path.
        #[arg(long, default_value = "target/soroban-gauge/report.json")]
        out: PathBuf,
    },
    /// Run, then write or update the baseline.
    Snapshot {
        /// Update only these scenarios (repeatable); other entries are kept.
        #[arg(long)]
        only: Vec<String>,
    },
    /// Compare two result files.
    Diff {
        /// Baseline file.
        #[arg(long)]
        baseline: PathBuf,
        /// Current result file.
        #[arg(long)]
        current: PathBuf,
        #[command(flatten)]
        compare: CompareArgs,
    },
    /// Run, then compare against the configured baseline (what CI calls).
    Check {
        /// Where to write the run report.
        #[arg(long, default_value = "target/soroban-gauge/report.json")]
        out: PathBuf,
        #[command(flatten)]
        compare: CompareArgs,
    },
    /// Compare local estimates with network simulation (milestone 2).
    Calibrate,
}

fn init_logging(cli: &Cli) {
    let level = if cli.quiet {
        tracing::Level::ERROR
    } else {
        match cli.verbose {
            0 => tracing::Level::WARN,
            1 => tracing::Level::INFO,
            2 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        }
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .init();
}

fn main() {
    let cli = Cli::parse();
    init_logging(&cli);
    let code = match cmds::dispatch(&cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            err.downcast_ref::<CoreError>()
                .map_or(5, CoreError::exit_code)
        }
    };
    std::process::exit(code);
}
