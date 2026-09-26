//! Unified `Rustrepo-sanitizer` executable.
//!
//! One binary provides the CLI, the Slint desktop GUI, and the Leptos/Axum web
//! UI. `--gui` and `--web` select the graphical interfaces (alone or together);
//! with neither, the sanitize CLI runs. No helper process is spawned.

use std::{io::Read, net::SocketAddr, path::PathBuf, process::ExitCode};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use itsulu_repo_sanitizer::help;
use itsulu_repo_sanitizer::sanitizer::{
    default_output_path, run, ArchiveFormat, Compression, Config, PasswordPolicy, ReportFormat,
};

const BIN: &str = "Rustrepo-sanitizer";

#[derive(Parser)]
#[command(
    name = "Rustrepo-sanitizer",
    version,
    max_term_width = 100,
    about = "Sanitize Git repositories for safe AI review (CLI, desktop GUI, and web UI)"
)]
struct Cli {
    #[arg(long, help = help::LAUNCH_GUI, help_heading = help::GROUP_LAUNCH)]
    gui: bool,
    #[arg(long, help = help::LAUNCH_WEB, help_heading = help::GROUP_LAUNCH)]
    web: bool,

    #[arg(long, value_name = "ADDR", help = help::WEB_BIND, help_heading = help::GROUP_WEB, requires = "web")]
    web_bind: Option<SocketAddr>,
    #[arg(long, value_name = "TOKEN", help = help::WEB_TOKEN, help_heading = help::GROUP_WEB, requires = "web")]
    web_token: Option<String>,
    #[arg(long, value_name = "DIR", help = help::WEB_ROOT, help_heading = help::GROUP_WEB, requires = "web")]
    web_root: Option<PathBuf>,
    #[arg(long, value_name = "PATHS", help = help::WEB_LOCAL_ROOTS, help_heading = help::GROUP_WEB, requires = "web")]
    web_local_roots: Option<String>,
    #[arg(long, value_name = "URL", help = help::WEB_FORGEJO_BASE, help_heading = help::GROUP_WEB, requires = "web")]
    web_forgejo_base: Option<String>,
    #[arg(long, value_name = "TOKEN", help = help::WEB_FORGEJO_TOKEN, help_heading = help::GROUP_WEB, requires = "web")]
    web_forgejo_token: Option<String>,
    #[arg(long, value_name = "URL", help = help::WEB_GITHUB_API, help_heading = help::GROUP_WEB, requires = "web")]
    web_github_api: Option<String>,
    #[arg(long, value_name = "TOKEN", help = help::WEB_GITHUB_TOKEN, help_heading = help::GROUP_WEB, requires = "web")]
    web_github_token: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        about = "Create a sanitized, reproducible review bundle from a Git repository",
        max_term_width = 100
    )]
    Sanitize(SanitizeArgs),
    #[command(about = "List supported archive and compression formats")]
    ListFormats,
}

#[derive(Args)]
struct SanitizeArgs {
    #[arg(default_value = ".", help = help::REPOSITORY, help_heading = help::GROUP_INPUT)]
    repository: PathBuf,
    #[arg(
        long,
        help = help::INCLUDE_UNTRACKED,
        help_heading = help::GROUP_INPUT
    )]
    include_untracked: bool,
    #[arg(short, long, help = help::OUTPUT, help_heading = help::GROUP_OUTPUT)]
    output: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::TIMESTAMP_NAME,
        help_heading = help::GROUP_OUTPUT
    )]
    timestamp_name: bool,
    #[arg(
        long,
        visible_alias = "format",
        value_enum,
        default_value_t = ArchiveFormat::Tar,
        help = help::ARCHIVE,
        help_heading = help::GROUP_ARCHIVE
    )]
    archive: ArchiveFormat,
    #[arg(
        long,
        value_enum,
        default_value_t = Compression::Zstd,
        help = help::COMPRESSION,
        help_heading = help::GROUP_ARCHIVE
    )]
    compression: Compression,
    #[arg(long = "include", help = help::INCLUDE, help_heading = help::GROUP_FILTERS)]
    include: Vec<String>,
    #[arg(long = "exclude", help = help::EXCLUDE, help_heading = help::GROUP_FILTERS)]
    exclude: Vec<String>,
    #[arg(
        long = "max-file-size",
        default_value = "10MiB",
        value_parser = parse_max_file_size_arg,
        help = help::MAX_FILE_SIZE,
        help_heading = help::GROUP_FILTERS
    )]
    max_file_size: u64,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::REDACT,
        help_heading = help::GROUP_REDACTION
    )]
    redact: bool,
    #[arg(
        long = "no-redact",
        action = clap::ArgAction::SetTrue,
        help = help::NO_REDACT,
        help_heading = help::GROUP_REDACTION
    )]
    no_redact: bool,
    #[arg(long, help = help::FAIL_ON_SECRET, help_heading = help::GROUP_REDACTION)]
    fail_on_secret: bool,
    #[arg(
        long,
        value_enum,
        default_value_t = CliReportFormat::Markdown,
        help = help::REPORT,
        help_heading = help::GROUP_REPORTS
    )]
    report: CliReportFormat,
    #[arg(long, help = help::DRY_RUN, help_heading = help::GROUP_REPORTS)]
    dry_run: bool,
    #[arg(
        long,
        conflicts_with = "password_stdin",
        help = help::PASSWORD_FILE,
        help_heading = help::GROUP_SECURITY
    )]
    password_file: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "password_file",
        help = help::PASSWORD_STDIN,
        help_heading = help::GROUP_SECURITY
    )]
    password_stdin: bool,
    #[arg(
        long = "password-min-length",
        default_value_t = 8,
        help = help::PASSWORD_MIN_LENGTH,
        help_heading = help::GROUP_SECURITY
    )]
    password_min_length: usize,
    #[arg(
        long = "password-require-uppercase",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_UPPERCASE,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_uppercase: bool,
    #[arg(
        long = "password-require-lowercase",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_LOWERCASE,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_lowercase: bool,
    #[arg(
        long = "password-require-number",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_NUMBER,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_number: bool,
    #[arg(
        long = "password-require-special",
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = help::PASSWORD_REQUIRE_SPECIAL,
        help_heading = help::GROUP_SECURITY
    )]
    password_require_special: bool,
    #[arg(short, long, help = help::VERBOSE, help_heading = help::GROUP_ADVANCED)]
    verbose: bool,
    #[arg(short, long, help = help::QUIET, help_heading = help::GROUP_ADVANCED)]
    quiet: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliReportFormat {
    Markdown,
    Json,
    None,
}

/// Accepts plain bytes or a binary unit (`2MiB`) for `--max-file-size`.
fn parse_max_file_size_arg(input: &str) -> Result<u64, String> {
    itsulu_repo_sanitizer::size::parse_size(input)
        .map(|size| size.bytes())
        .map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if (cli.gui || cli.web) && cli.command.is_some() {
        eprintln!("{BIN}: --gui/--web cannot be combined with a subcommand");
        return ExitCode::from(2);
    }
    if cli.gui || cli.web {
        return launch(&cli);
    }

    match cli.command {
        Some(Command::ListFormats) => {
            itsulu_repo_sanitizer::sanitizer::print_formats();
            ExitCode::SUCCESS
        }
        Some(Command::Sanitize(args)) => run_sanitize(args),
        None => {
            // No launch mode and no subcommand: show help on stderr.
            let mut command = Cli::command();
            let _ = command.print_help();
            ExitCode::from(2)
        }
    }
}

#[cfg(feature = "web")]
fn web_settings(cli: &Cli) -> itsulu_repo_sanitizer::web::state::WebSettings {
    let mut settings = itsulu_repo_sanitizer::web::state::WebSettings::from_env();
    if let Some(bind) = cli.web_bind {
        settings.bind = bind;
    }
    if let Some(token) = &cli.web_token {
        settings.token = Some(token.clone());
    }
    if let Some(root) = &cli.web_root {
        settings.root = root.clone();
    }
    if let Some(roots) = &cli.web_local_roots {
        settings.local_roots = roots
            .split(':')
            .filter(|entry| !entry.trim().is_empty())
            .map(PathBuf::from)
            .collect();
    }
    if let Some(base) = &cli.web_forgejo_base {
        settings.forgejo_base = url::Url::parse(base).ok();
    }
    if let Some(token) = &cli.web_forgejo_token {
        settings.forgejo_token = Some(token.clone());
    }
    if let Some(api) = &cli.web_github_api {
        settings.github_api = url::Url::parse(api).ok();
    }
    if let Some(token) = &cli.web_github_token {
        settings.github_token = Some(token.clone());
    }
    settings
}

fn launch(cli: &Cli) -> ExitCode {
    // Reject a requested mode that this build cannot provide, rather than
    // silently starting only the available one.
    if cli.gui && !cfg!(feature = "gui") {
        eprintln!("{BIN}: this build has no GUI support enabled");
        return ExitCode::from(2);
    }
    if cli.web && !cfg!(feature = "web") {
        eprintln!("{BIN}: this build has no web support enabled");
        return ExitCode::from(2);
    }
    #[cfg(all(feature = "gui", feature = "web"))]
    if cli.gui && cli.web {
        return run_gui_and_web(web_settings(cli));
    }
    #[cfg(feature = "gui")]
    if cli.gui {
        return match itsulu_repo_sanitizer::gui::run_gui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("{BIN}: GUI error: {err}");
                ExitCode::from(1)
            }
        };
    }
    #[cfg(feature = "web")]
    if cli.web {
        return match itsulu_repo_sanitizer::web::server::run_web_only(web_settings(cli)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("{BIN}: web error: {err:#}");
                ExitCode::from(1)
            }
        };
    }
    let _ = cli;
    eprintln!("{BIN}: no interface selected");
    ExitCode::from(2)
}

/// Runs the GUI on the main thread and the web server on a background thread,
/// so neither interface blocks the other. Closing the GUI shuts the server down,
/// and an interrupting signal stops both.
#[cfg(all(feature = "gui", feature = "web"))]
fn run_gui_and_web(settings: itsulu_repo_sanitizer::web::state::WebSettings) -> ExitCode {
    let on_signal: Option<itsulu_repo_sanitizer::web::server::SignalCallback> =
        Some(Box::new(|| {
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        }));
    run_gui_and_web_with(settings, on_signal, || {
        itsulu_repo_sanitizer::gui::run_gui().map_err(|err| err.to_string())
    })
}

/// The composition behind GUI + Web, with an injectable GUI runner so the
/// "closing the GUI stops the web server" contract is testable without a display.
#[cfg(all(feature = "gui", feature = "web"))]
fn run_gui_and_web_with(
    settings: itsulu_repo_sanitizer::web::state::WebSettings,
    on_signal: Option<itsulu_repo_sanitizer::web::server::SignalCallback>,
    run_gui: impl FnOnce() -> Result<(), String>,
) -> ExitCode {
    let (handle, shutdown) =
        match itsulu_repo_sanitizer::web::server::spawn_web(settings, on_signal) {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!("{BIN}: web error: {err:#}");
                return ExitCode::from(1);
            }
        };
    let gui_result = run_gui();
    let _ = shutdown.send(());
    // Bound the graceful drain so a stalled connection cannot keep the process
    // alive after the GUI closes.
    let web_result = join_with_timeout(handle, std::time::Duration::from_secs(20));
    match (gui_result, web_result) {
        (Ok(()), Some(Ok(Ok(())))) => ExitCode::SUCCESS,
        (gui, web) => {
            if let Err(err) = gui {
                eprintln!("{BIN}: GUI error: {err}");
            }
            match web {
                Some(Ok(Err(err))) => eprintln!("{BIN}: web error: {err:#}"),
                Some(Err(_)) => eprintln!("{BIN}: web server thread panicked"),
                None => eprintln!("{BIN}: web server did not stop within the timeout"),
                Some(Ok(Ok(()))) => {}
            }
            ExitCode::from(1)
        }
    }
}

#[cfg(all(feature = "gui", feature = "web"))]
fn join_with_timeout(
    handle: std::thread::JoinHandle<anyhow::Result<()>>,
    timeout: std::time::Duration,
) -> Option<std::thread::Result<anyhow::Result<()>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    rx.recv_timeout(timeout).ok()
}

fn run_sanitize(args: SanitizeArgs) -> ExitCode {
    let format = args.archive;
    let report = match args.report {
        CliReportFormat::Markdown => ReportFormat::Markdown,
        CliReportFormat::Json => ReportFormat::Json,
        CliReportFormat::None => ReportFormat::None,
    };
    let output = match args.output {
        Some(path) => path,
        None => match default_output_path(
            &args.repository,
            format,
            args.compression,
            args.timestamp_name,
        ) {
            Ok(path) => path,
            Err(err) => {
                eprintln!("{BIN}: {err:#}");
                return ExitCode::from(if err.to_string().contains("compression") {
                    2
                } else {
                    3
                });
            }
        },
    };
    let password = match read_password(args.password_file.as_deref(), args.password_stdin) {
        Ok(password) => password,
        Err(err) => {
            eprintln!("{BIN}: {err}");
            return ExitCode::from(2);
        }
    };
    let config = Config {
        repository: args.repository,
        output,
        format,
        compression: args.compression,
        report,
        include_untracked: args.include_untracked,
        max_file_size: args.max_file_size,
        excludes: args.exclude,
        includes: args.include,
        redact: if args.no_redact { false } else { args.redact },
        fail_on_secret: args.fail_on_secret,
        dry_run: args.dry_run,
        password,
        password_policy: PasswordPolicy {
            minimum_length: args.password_min_length,
            require_uppercase: args.password_require_uppercase,
            require_lowercase: args.password_require_lowercase,
            require_number: args.password_require_number,
            require_special: args.password_require_special,
        },
        password_file: args.password_file,
        verbose: args.verbose,
        quiet: args.quiet,
    };
    match run(config) {
        Ok(summary) => {
            if !summary.quiet {
                let limit = itsulu_repo_sanitizer::size::Size::from_bytes(args.max_file_size);
                println!(
                    "sanitized {} files ({} excluded, {} redactions, max file size {}){}",
                    summary.included,
                    summary.excluded,
                    summary.redactions,
                    limit,
                    if summary.dry_run { "; dry run" } else { "" }
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{BIN}: {err:#}");
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

fn read_password(
    path: Option<&std::path::Path>,
    from_stdin: bool,
) -> Result<Option<String>, String> {
    let value = if let Some(path) = path {
        std::fs::read_to_string(path).map_err(|_| "unable to read password file".to_owned())?
    } else if from_stdin {
        let mut value = String::new();
        std::io::stdin()
            .read_to_string(&mut value)
            .map_err(|_| "unable to read password from stdin".to_owned())?;
        value
    } else {
        return Ok(None);
    };
    let value = value.strip_suffix('\n').unwrap_or(&value);
    let value = value.strip_suffix('\r').unwrap_or(value).to_owned();
    if value.is_empty() {
        Err("password must not be empty".to_owned())
    } else {
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_parser_preserves_safe_defaults_and_explicit_options() {
        let cli = Cli::try_parse_from([
            BIN,
            "sanitize",
            "repo",
            "--archive",
            "none",
            "--compression",
            "gzip",
            "--report",
            "json",
            "--include",
            "src/**",
            "--exclude",
            "target/**",
            "--include-untracked",
            "--dry-run",
            "--no-redact",
        ])
        .unwrap();
        let Command::Sanitize(args) = cli.command.unwrap() else {
            panic!("expected sanitize command")
        };
        assert_eq!(args.repository, PathBuf::from("repo"));
        assert_eq!(args.archive, ArchiveFormat::None);
        assert_eq!(args.compression, Compression::Gzip);
        assert!(matches!(args.report, CliReportFormat::Json));
        assert_eq!(args.include, vec!["src/**"]);
        assert_eq!(args.exclude, vec!["target/**"]);
        assert!(args.include_untracked && args.dry_run && args.no_redact && args.redact);
    }

    #[test]
    fn password_file_and_stdin_are_mutually_exclusive() {
        assert!(Cli::try_parse_from([
            BIN,
            "sanitize",
            "--password-file",
            "password.txt",
            "--password-stdin",
        ])
        .is_err());
    }

    #[test]
    fn sanitize_parser_uses_documented_defaults() {
        let cli = Cli::try_parse_from([BIN, "sanitize"]).unwrap();
        let Command::Sanitize(args) = cli.command.unwrap() else {
            panic!("expected sanitize command")
        };
        assert_eq!(args.repository, PathBuf::from("."));
        assert_eq!(args.archive, ArchiveFormat::Tar);
        assert_eq!(args.compression, Compression::Zstd);
        assert!(matches!(args.report, CliReportFormat::Markdown));
        assert!(args.redact && args.timestamp_name);
        assert_eq!(args.password_min_length, 8);
    }

    #[test]
    fn launch_modes_parse_and_web_options_require_web() {
        let cli = Cli::try_parse_from([BIN, "--gui"]).unwrap();
        assert!(cli.gui && !cli.web);
        let cli = Cli::try_parse_from([BIN, "--web", "--web-bind", "127.0.0.1:9000"]).unwrap();
        assert!(cli.web && !cli.gui);
        assert_eq!(cli.web_bind.unwrap().port(), 9000);
        let cli = Cli::try_parse_from([BIN, "--gui", "--web"]).unwrap();
        assert!(cli.gui && cli.web);
        // Web options are only valid together with --web.
        assert!(Cli::try_parse_from([BIN, "--web-bind", "127.0.0.1:9000"]).is_err());
    }

    #[test]
    fn unavailable_modes_are_reported_rather_than_downgraded() {
        if !cfg!(feature = "gui") {
            let cli = Cli::try_parse_from([BIN, "--gui"]).unwrap();
            assert_eq!(launch(&cli), ExitCode::from(2));
        }
        if !cfg!(feature = "web") {
            let cli = Cli::try_parse_from([BIN, "--web"]).unwrap();
            assert_eq!(launch(&cli), ExitCode::from(2));
        }
    }

    /// The GUI + Web contract: the web server runs on a background thread while
    /// the GUI runs, and the server stops when the GUI returns.
    #[cfg(all(feature = "gui", feature = "web"))]
    #[test]
    fn gui_and_web_runs_both_and_stops_the_server_when_the_gui_exits() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let root = tempfile::tempdir().unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let settings = itsulu_repo_sanitizer::web::state::WebSettings {
            bind: format!("127.0.0.1:{port}").parse().unwrap(),
            root: root.path().to_path_buf(),
            ..Default::default()
        };
        let code = run_gui_and_web_with(settings, None, || {
            // Stand in for the GUI event loop: confirm the server is up, then
            // return as if the window had been closed.
            for _ in 0..100 {
                if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                    let _ = stream.write_all(
                        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                    );
                    let mut buffer = String::new();
                    let _ = stream.read_to_string(&mut buffer);
                    if buffer.contains("200") {
                        return Ok(());
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err("web server was not healthy while the GUI ran".to_owned())
        });
        assert_eq!(code, ExitCode::SUCCESS);
        // The port is released once the GUI has exited.
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
    }
}
