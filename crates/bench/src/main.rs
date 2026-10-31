use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use bench::{
    check_paths, idle_cpu_regressed, missing_baselines, read_document, scenarios, time_command,
    write_document, IdleComparison, ScenarioId, TargetId, TimestampLog,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bc-bench")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the six scenarios.
    Scenarios,
    /// Write the empty Wallpaper Engine and Lively baseline.
    Placeholder {
        #[arg(long, default_value = "results/baselines.json")]
        output: PathBuf,
    },
    /// Fail when a candidate idle-static CPU mean is above the baseline.
    Check {
        #[arg(long, default_value = "results/baselines.json")]
        baseline: PathBuf,
        #[arg(long)]
        candidate: Option<PathBuf>,
    },
    /// Record one scenario. Does nothing numeric unless the target process is running.
    Record {
        #[arg(long)]
        target: String,
        #[arg(long)]
        scenario: String,
        #[arg(long)]
        duration_secs: Option<u64>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        timestamps: Option<PathBuf>,
        #[arg(long)]
        presentmon_csv: Option<PathBuf>,
    },
    /// Time process creation for a program, then stop it.
    ColdStart {
        program: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), bench::BenchError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Scenarios => {
            for scenario in scenarios() {
                println!(
                    "{}\t{}s\t{}",
                    scenario.id.as_str(),
                    scenario.duration.as_secs(),
                    scenario.summary
                );
            }
            Ok(())
        }
        Commands::Placeholder { output } => {
            write_document(&output, &missing_baselines())?;
            println!("wrote {}", output.display());
            Ok(())
        }
        Commands::Check {
            baseline,
            candidate,
        } => {
            let comparisons = check_paths(&baseline, candidate.as_deref())?;
            if comparisons.is_empty() {
                let document = read_document(&baseline)?;
                println!(
                    "no candidate run; baseline {} has {} target(s) and is not an idle-CPU regression",
                    baseline.display(),
                    document.targets.len()
                );
                return Ok(());
            }
            for comparison in &comparisons {
                match comparison {
                    IdleComparison::Skipped { target, reason } => {
                        println!("{} skipped: {reason}", target.as_str());
                    }
                    IdleComparison::WithinBaseline {
                        target,
                        baseline,
                        candidate,
                    } => {
                        println!(
                            "{} idle CPU {candidate} is within baseline {baseline}",
                            target.as_str()
                        );
                    }
                    IdleComparison::Increased {
                        target,
                        baseline,
                        candidate,
                    } => {
                        eprintln!(
                            "{} idle CPU increased from {baseline} to {candidate}",
                            target.as_str()
                        );
                    }
                }
            }
            if idle_cpu_regressed(&comparisons) {
                Err(bench::BenchError::Message(
                    "idle CPU regression".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        Commands::Record {
            target,
            scenario,
            duration_secs,
            output,
            timestamps,
            presentmon_csv,
        } => {
            let target = TargetId::parse(&target)?;
            let scenario = ScenarioId::parse(&scenario)?;
            let timestamps = match timestamps {
                Some(path) => {
                    let text = std::fs::read_to_string(&path)?;
                    Some(serde_json::from_str::<TimestampLog>(&text)?)
                }
                None => None,
            };
            let csv = match presentmon_csv {
                Some(path) => Some(std::fs::read_to_string(path)?),
                None => None,
            };
            let document = bench::record_to_file(
                target,
                scenario,
                duration_secs.map(Duration::from_secs),
                timestamps.as_ref(),
                csv.as_deref(),
                &output,
            )?;
            println!("wrote {} ({:?})", output.display(), document.kind);
            Ok(())
        }
        Commands::ColdStart { program, args } => {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let elapsed = time_command(&program, &arg_refs)?;
            println!("cold_start_ms={}", elapsed.as_millis());
            Ok(())
        }
    }
}
