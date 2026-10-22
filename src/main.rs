use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use rust1100_check::report::{self, Report};
use rust1100_check::{Severity, beta, cooldown, fix_yaml, scan_static};

#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cli {
    #[command(name = "rust1100-check", version, about)]
    Check(Args),
}

#[derive(clap::Args)]
struct Args {
    /// Repository root to scan
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Preview Cargo's cooldown with this age, e.g. "7 days" (fetches from the crates.io index)
    #[arg(long, value_name = "AGE")]
    min_publish_age: Option<String>,
    /// Run `cargo check` with the baseline toolchain and beta, and report diagnostics only beta emits
    #[arg(long)]
    beta_diff: bool,
    /// Quote the unquoted YAML versions in place, then report what is left
    #[arg(long)]
    fix: bool,
    /// Toolchain that `--beta-diff` compares against
    #[arg(long, value_name = "TOOLCHAIN", default_value = "stable")]
    baseline: String,
    /// Lowest severity that makes the exit code 1
    #[arg(long, value_enum, default_value_t = FailOn::Error)]
    fail_on: FailOn,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
    Github,
    Sarif,
}

#[derive(Clone, Copy, ValueEnum)]
enum FailOn {
    Error,
    Warning,
    Note,
    Never,
}

fn run(args: Args) -> Result<bool, String> {
    let root = args.path;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let (mut findings, mut files_scanned) = scan_static(&root)?;
    if args.fix {
        let fixed = fix_yaml(&root, &findings)?;
        eprintln!("quoted {fixed} YAML version value(s)");
        (findings, files_scanned) = scan_static(&root)?;
    }
    let mut cooldown_summary = None;
    if let Some(age) = &args.min_publish_age {
        let (found, summary) = cooldown::run(&root, age)?;
        findings.extend(found);
        cooldown_summary = Some(summary);
    }
    let mut toolchains = None;
    if args.beta_diff {
        let (found, stable, beta) = beta::run(&root, &args.baseline)?;
        findings.extend(found);
        toolchains = Some((stable, beta));
    }
    let threshold = match args.fail_on {
        FailOn::Error => Some(Severity::Error),
        FailOn::Warning => Some(Severity::Warning),
        FailOn::Note => Some(Severity::Note),
        FailOn::Never => None,
    };
    let failed = threshold.is_some_and(|t| findings.iter().any(|f| f.severity >= t));
    let report = Report {
        findings,
        files_scanned,
        cooldown: cooldown_summary,
        toolchains,
    };
    let rendered = match args.format {
        Format::Text => report::text(&report),
        Format::Json => report::json(&report),
        Format::Github => report::github(&report),
        Format::Sarif => report::sarif(&report),
    };
    print!("{rendered}");
    Ok(failed)
}

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().collect();
    // Allow running the binary directly as well as `cargo rust1100-check`.
    if argv.get(1).map(String::as_str) != Some("rust1100-check") {
        argv.insert(1, "rust1100-check".to_owned());
    }
    let Cli::Check(args) = Cli::parse_from(argv);
    match run(args) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
