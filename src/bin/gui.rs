#[cfg(feature = "gui")]
slint::include_modules!();

#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use itsulu_repo_sanitizer::sanitizer::{
        default_output_path, run_with_progress, validate_config, ArchiveFormat, Compression,
        Config, ProgressEvent, ReportFormat,
    };
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let window = MainWindow::new()?;
    let weak = window.as_weak();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_ui = weak.clone();
    let cancel_for_callback = cancel_token.clone();
    window.on_cancel(move || {
        cancel_for_callback.store(true, Ordering::Relaxed);
        let _ = slint::invoke_from_event_loop({
            let cancel_ui = cancel_ui.clone();
            move || {
                if let Some(window) = cancel_ui.upgrade() {
                    window.set_status("Cancelling…".into());
                }
            }
        });
    });
    let browse_ui = weak.clone();
    window.on_browse(move || {
        let browse_ui = browse_ui.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose Git repository")
                .pick_folder()
            {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = browse_ui.upgrade() {
                        window.set_repository_path(path.display().to_string().into());
                        window
                            .set_status(format!("Selected repository: {}", path.display()).into());
                    }
                });
            }
        });
    });
    let result_ui = weak.clone();
    window.on_open_result(move || {
        if let Some(window) = result_ui.upgrade() {
            let path = window.get_result_path().to_string();
            if let Some(parent) = PathBuf::from(path).parent() {
                let _ = webbrowser::open(parent.to_string_lossy().as_ref());
            }
        }
    });
    window.on_sanitize(
        move |repo,
              output,
              untracked,
              dry_run,
              format_index,
              compression_index,
              report_index,
              timestamp,
              redact,
              fail_secret,
              max_size,
              include,
              exclude,
              password| {
            let repo = PathBuf::from(repo.to_string());
            let format = match format_index {
                1 => ArchiveFormat::Zip,
                2 => ArchiveFormat::SevenZip,
                _ => ArchiveFormat::Tar,
            };
            let compression = match compression_index {
                1 => Compression::Gzip,
                2 => Compression::None,
                _ => Compression::Zstd,
            };
            let report = match report_index {
                1 => ReportFormat::Json,
                2 => ReportFormat::None,
                _ => ReportFormat::Markdown,
            };
            let output = if output.is_empty() {
                default_output_path(&repo, format, compression, timestamp)
                    .unwrap_or_else(|_| PathBuf::from("sanitized.tar.zst"))
            } else {
                PathBuf::from(output.to_string())
            };
            let config = Config {
                repository: repo,
                output,
                format,
                compression,
                report,
                include_untracked: untracked,
                max_file_size: max_size.parse().unwrap_or(10 * 1024 * 1024),
                excludes: if exclude.is_empty() {
                    vec![]
                } else {
                    vec![exclude.to_string()]
                },
                includes: if include.is_empty() {
                    vec![]
                } else {
                    vec![include.to_string()]
                },
                redact,
                fail_on_secret: fail_secret,
                dry_run,
                password: if password.is_empty() {
                    None
                } else {
                    Some(password.to_string())
                },
                password_file: None,
                verbose: false,
                quiet: true,
            };
            if let Err(error) = validate_config(&config) {
                let validation_ui = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = validation_ui.upgrade() {
                        window.set_status(format!("Invalid options: {error}").into());
                    }
                });
                return;
            }
            let ui = weak.clone();
            let final_ui = weak.clone();
            let cancel = cancel_token.clone();
            cancel.store(false, Ordering::Relaxed);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = ui.upgrade() {
                    window.set_running(true);
                    window.set_status("Sanitizing…".into());
                }
            });
            std::thread::spawn(move || {
                let progress_ui = final_ui.clone();
                let result_path = config.output.clone();
                let result = run_with_progress(
                    config,
                    move |event| {
                        if let ProgressEvent::Scanning { examined, .. } = event {
                            let progress_ui = progress_ui.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = progress_ui.upgrade() {
                                    w.set_status(format!("Scanning… {examined} files").into());
                                }
                            });
                        }
                    },
                    || cancel.load(Ordering::Relaxed),
                );
                let status = match result {
                    Ok(s) => format!(
                        "Complete: {} files, {} redactions",
                        s.included, s.redactions
                    ),
                    Err(e) => format!("Error: {e:#}"),
                };
                let succeeded = status.starts_with("Complete:");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = final_ui.upgrade() {
                        window.set_running(false);
                        if succeeded {
                            window.set_result_path(result_path.display().to_string().into());
                        }
                        window.set_status(status.into());
                    }
                });
            });
        },
    );
    window.run()?;
    Ok(())
}
#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("GUI support is disabled; run with --features gui");
    std::process::exit(2);
}
