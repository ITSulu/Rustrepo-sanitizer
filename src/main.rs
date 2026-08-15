mod sanitizer;
mod security;

use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use sanitizer::{default_output_path, run, Config, ReportFormat};

#[derive(Parser)]
#[command(
    name = "itsulu-repo-sanitizer",
    version,
    about = "Create a safe AI review archive from a Git repository"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Sanitize(SanitizeArgs),
}

#[derive(Args)]
struct SanitizeArgs {
    #[arg(default_value = ".")]
    repository: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = ArchiveFormat::TarZst)]
    format: ArchiveFormat,
    #[arg(long, value_enum, default_value_t = CliReportFormat::Markdown)]
    report: CliReportFormat,
    #[arg(long)]
    include_untracked: bool,
    #[arg(long = "max-file-size", default_value_t = 10 * 1024 * 1024)]
    max_file_size: u64,
    #[arg(long = "exclude")]
    exclude: Vec<String>,
    #[arg(long = "include")]
    include: Vec<String>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    redact: bool,
    #[arg(long = "no-redact", action = clap::ArgAction::SetTrue)]
    no_redact: bool,
    #[arg(long)]
    fail_on_secret: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(short, long)]
    verbose: bool,
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ArchiveFormat {
    #[value(name = "tar.gz")]
    TarGz,
    #[value(name = "tar.zst")]
    TarZst,
}
#[derive(Clone, Copy, ValueEnum)]
enum CliReportFormat {
    Markdown,
    Json,
    None,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Command::Sanitize(args) = cli.command;
    let format = match args.format {
        ArchiveFormat::TarGz => sanitizer::ArchiveFormat::TarGz,
        ArchiveFormat::TarZst => sanitizer::ArchiveFormat::TarZst,
    };
    let report = match args.report {
        CliReportFormat::Markdown => ReportFormat::Markdown,
        CliReportFormat::Json => ReportFormat::Json,
        CliReportFormat::None => ReportFormat::None,
    };
    let output = match args.output {
        Some(path) => path,
        None => match default_output_path(&args.repository, format) {
            Ok(path) => path,
            Err(err) => {
                eprintln!("itsulu-repo-sanitizer: {err:#}");
                return ExitCode::from(3);
            }
        },
    };
    let config = Config {
        repository: args.repository,
        output,
        format,
        report,
        include_untracked: args.include_untracked,
        max_file_size: args.max_file_size,
        excludes: args.exclude,
        includes: args.include,
        redact: if args.no_redact { false } else { args.redact },
        fail_on_secret: args.fail_on_secret,
        dry_run: args.dry_run,
        verbose: args.verbose,
        quiet: args.quiet,
    };
    match run(config) {
        Ok(summary) => {
            if !summary.quiet {
                println!(
                    "sanitized {} files ({} excluded, {} redactions){}",
                    summary.included,
                    summary.excluded,
                    summary.redactions,
                    if summary.dry_run { "; dry run" } else { "" }
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("itsulu-repo-sanitizer: {err:#}");
            let message = err.to_string().to_ascii_lowercase();
            let code = if message.contains("secret detected") {
                4
            } else if message.contains("creating ") || message.contains("output archive") {
                5
            } else if message.contains("git ") || message.contains("working tree") {
                3
            } else {
                2
            };
            ExitCode::from(code)
        }
    }
}
