#[cfg(feature = "gui")]
slint::include_modules!();

#[allow(dead_code)]
fn result_path_for_outcome(output: &std::path::Path, succeeded: bool) -> String {
    if succeeded {
        output.display().to_string()
    } else {
        String::new()
    }
}

fn output_path_for_capability_change(current: &str, automatic: bool, extension: &str) -> String {
    if !automatic || current.is_empty() {
        return current.to_owned();
    }
    let mut path = std::path::PathBuf::from(current);
    path.set_extension(extension);
    path.display().to_string()
}

#[allow(dead_code)]
fn settings_values_for_policy(
    policy: &crate::security::PasswordPolicy,
) -> (String, bool, bool, bool, bool) {
    (
        policy.minimum_length.to_string(),
        policy.require_uppercase,
        policy.require_lowercase,
        policy.require_number,
        policy.require_special,
    )
}

#[cfg(feature = "gui")]
fn refresh_output_extension(window: &MainWindow, format_index: i32, compression_index: i32) {
    use crate::sanitizer::{compression_for_gui_selection, output_extension, ArchiveFormat};
    let format = match format_index {
        1 => ArchiveFormat::Zip,
        2 => ArchiveFormat::SevenZip,
        3 => ArchiveFormat::None,
        _ => ArchiveFormat::Tar,
    };
    if let Some(compression) = compression_for_gui_selection(format, compression_index as usize) {
        if let Ok(extension) = output_extension(format, compression) {
            let current = window.get_output_path();
            let updated = output_path_for_capability_change(
                current.as_str(),
                window.get_output_path_automatic(),
                &extension,
            );
            if updated != current.as_str() {
                window.set_output_path(updated.into());
            }
        }
    }
}

pub fn run_gui() -> Result<(), Box<dyn std::error::Error>> {
    use crate::sanitizer::{
        add_pattern, compatible_compressions, compression_capability,
        compression_for_gui_selection, default_output_path, remove_pattern, run_with_progress,
        validate_config, ArchiveFormat, Compression, Config, PasswordPolicy, ProgressEvent,
        ReportFormat,
    };
    use slint::{ComponentHandle, Model, ModelRc, VecModel};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let window = MainWindow::new()?;
    window.set_app_title(format!("Rustrepo Sanitizer {}", env!("CARGO_PKG_VERSION")).into());
    if let Ok(repository) = std::env::var("RRS_GUI_REPOSITORY") {
        window.set_repository_path(repository.into());
    }
    let set_compression_options = |window: &MainWindow, format_index: i32| {
        let format = match format_index {
            1 => ArchiveFormat::Zip,
            2 => ArchiveFormat::SevenZip,
            3 => ArchiveFormat::None,
            _ => ArchiveFormat::Tar,
        };
        let options = compatible_compressions(format)
            .into_iter()
            .map(|compression| {
                if format == ArchiveFormat::SevenZip && compression == Compression::None {
                    "7z".into()
                } else {
                    compression_capability(compression).label.into()
                }
            })
            .collect::<Vec<slint::SharedString>>();
        window.set_compression_options(ModelRc::new(VecModel::from(options)));
    };
    set_compression_options(&window, 0);
    let compression_ui = window.as_weak();
    window.on_archive_changed(move |format_index| {
        if let Some(window) = compression_ui.upgrade() {
            set_compression_options(&window, format_index);
            refresh_output_extension(&window, format_index, 0);
        }
    });
    let output_ui = window.as_weak();
    window.on_compression_changed(move |format_index, compression_index| {
        if let Some(window) = output_ui.upgrade() {
            refresh_output_extension(&window, format_index, compression_index);
        }
    });
    let include_ui = window.as_weak();
    window.on_add_include(move |pattern| {
        if let Some(window) = include_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_include_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            match add_pattern(&mut patterns, pattern.as_str()) {
                Ok(true) => {
                    window.set_include_patterns(ModelRc::new(VecModel::from(
                        patterns
                            .iter()
                            .map(|pattern| pattern.clone().into())
                            .collect::<Vec<slint::SharedString>>(),
                    )));
                    window.set_include_glob(patterns.join("\n").into());
                }
                Ok(false) => window.set_status("Include pattern already selected".into()),
                Err(error) => window.set_status(format!("Invalid include glob: {error:#}").into()),
            }
        }
    });
    let include_remove_ui = window.as_weak();
    window.on_remove_include(move |pattern| {
        if let Some(window) = include_remove_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_include_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            if remove_pattern(&mut patterns, pattern.as_str()) {
                window.set_include_patterns(ModelRc::new(VecModel::from(
                    patterns
                        .iter()
                        .map(|pattern| pattern.clone().into())
                        .collect::<Vec<slint::SharedString>>(),
                )));
                window.set_include_glob(patterns.join("\n").into());
            }
        }
    });
    let exclude_ui = window.as_weak();
    window.on_add_exclude(move |pattern| {
        if let Some(window) = exclude_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_exclude_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            match add_pattern(&mut patterns, pattern.as_str()) {
                Ok(true) => {
                    window.set_exclude_patterns(ModelRc::new(VecModel::from(
                        patterns
                            .iter()
                            .map(|pattern| pattern.clone().into())
                            .collect::<Vec<slint::SharedString>>(),
                    )));
                    window.set_exclude_glob(patterns.join("\n").into());
                }
                Ok(false) => window.set_status("Exclude pattern already selected".into()),
                Err(error) => window.set_status(format!("Invalid exclude glob: {error:#}").into()),
            }
        }
    });
    let exclude_remove_ui = window.as_weak();
    window.on_remove_exclude(move |pattern| {
        if let Some(window) = exclude_remove_ui.upgrade() {
            let mut patterns: Vec<String> = window
                .get_exclude_patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect();
            if remove_pattern(&mut patterns, pattern.as_str()) {
                window.set_exclude_patterns(ModelRc::new(VecModel::from(
                    patterns
                        .iter()
                        .map(|pattern| pattern.clone().into())
                        .collect::<Vec<slint::SharedString>>(),
                )));
                window.set_exclude_glob(patterns.join("\n").into());
            }
        }
    });
    let help_window = Rc::new(HelpWindow::new()?);
    let settings_window = Rc::new(SettingsWindow::new()?);
    let about_window = Rc::new(AboutWindow::new()?);
    about_window.set_version(env!("CARGO_PKG_VERSION").into());
    about_window.set_build_date(
        option_env!("SOURCE_DATE_EPOCH")
            .map(|value| format!("SOURCE_DATE_EPOCH={value}"))
            .unwrap_or_else(|| "reproducible build metadata unavailable".to_owned())
            .into(),
    );
    let help_for_callback = help_window.clone();
    window.on_show_help(move || {
        let _ = help_for_callback.show();
    });
    let settings_for_callback = settings_window.clone();
    let settings_source = window.as_weak();
    window.on_show_settings(move || {
        if let Some(window) = settings_source.upgrade() {
            let policy = PasswordPolicy {
                minimum_length: window
                    .get_password_policy_minimum_length()
                    .parse()
                    .unwrap_or(8),
                require_uppercase: window.get_password_policy_uppercase(),
                require_lowercase: window.get_password_policy_lowercase(),
                require_number: window.get_password_policy_number(),
                require_special: window.get_password_policy_special(),
            };
            let (minimum, uppercase, lowercase, number, special) =
                settings_values_for_policy(&policy);
            settings_for_callback.set_minimum_length(minimum.into());
            settings_for_callback.set_require_uppercase(uppercase);
            settings_for_callback.set_require_lowercase(lowercase);
            settings_for_callback.set_require_number(number);
            settings_for_callback.set_require_special(special);
        }
        let _ = settings_for_callback.show();
    });
    let policy_main = window.as_weak();
    let policy_settings = settings_window.clone();
    settings_window.on_apply_policy(move || {
        if let Some(window) = policy_main.upgrade() {
            window.set_password_policy_minimum_length(policy_settings.get_minimum_length());
            window.set_password_policy_uppercase(policy_settings.get_require_uppercase());
            window.set_password_policy_lowercase(policy_settings.get_require_lowercase());
            window.set_password_policy_number(policy_settings.get_require_number());
            window.set_password_policy_special(policy_settings.get_require_special());
        }
    });
    let about_for_callback = about_window.clone();
    window.on_show_about(move || {
        let _ = about_for_callback.show();
    });
    let about_links = about_window.clone();
    about_links.on_open_website(move |url| {
        let _ = webbrowser::open(url.as_str());
    });
    window.on_quit_requested(|| {
        let _ = slint::quit_event_loop();
    });
    let cancel_shortcut_ui = window.as_weak();
    window.on_cancel_shortcut(move || {
        if let Some(window) = cancel_shortcut_ui.upgrade() {
            window.invoke_cancel();
        }
    });
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
                        if window.get_output_path().is_empty() {
                            if let Ok(output) = default_output_path(
                                &path,
                                ArchiveFormat::Tar,
                                Compression::Zstd,
                                true,
                            ) {
                                window.set_output_path(output.display().to_string().into());
                            }
                        }
                        window
                            .set_status(format!("Selected repository: {}", path.display()).into());
                    }
                });
            }
        });
    });
    let output_ui = weak.clone();
    window.on_browse_output(move |repo, format_index, compression_index, timestamp| {
        let output_ui = output_ui.clone();
        let repo = PathBuf::from(repo.to_string());
        std::thread::spawn(move || {
            if let Some(directory) = rfd::FileDialog::new()
                .set_title("Choose output folder")
                .pick_folder()
            {
                let format = match format_index {
                    1 => ArchiveFormat::Zip,
                    2 => ArchiveFormat::SevenZip,
                    3 => ArchiveFormat::None,
                    _ => ArchiveFormat::Tar,
                };
                let Some(compression) =
                    compression_for_gui_selection(format, compression_index as usize)
                else {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = output_ui.upgrade() {
                            window.set_status("Invalid compression for selected archive".into());
                        }
                    });
                    return;
                };
                let filename = default_output_path(&repo, format, compression, timestamp)
                    .ok()
                    .and_then(|path| path.file_name().map(|name| name.to_owned()))
                    .unwrap_or_else(|| "sanitized.tar.zst".into());
                let path = directory.join(filename);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = output_ui.upgrade() {
                        window.set_output_path_automatic(false);
                        window.set_status(
                            format!("Output folder selected: {}", directory.display()).into(),
                        );
                        window.set_output_path(path.display().to_string().into());
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
                3 => ArchiveFormat::None,
                _ => ArchiveFormat::Tar,
            };
            let Some(compression) =
                compression_for_gui_selection(format, compression_index as usize)
            else {
                if let Some(window) = weak.upgrade() {
                    window.set_status("Invalid compression for selected archive".into());
                }
                return;
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
            let password_policy = weak
                .upgrade()
                .map(|window| PasswordPolicy {
                    minimum_length: window
                        .get_password_policy_minimum_length()
                        .parse()
                        .unwrap_or(8),
                    require_uppercase: window.get_password_policy_uppercase(),
                    require_lowercase: window.get_password_policy_lowercase(),
                    require_number: window.get_password_policy_number(),
                    require_special: window.get_password_policy_special(),
                })
                .unwrap_or_default();
            let config = Config {
                repository: repo,
                output,
                format,
                compression,
                report,
                include_untracked: untracked,
                max_file_size: max_size.parse().unwrap_or(10 * 1024 * 1024),
                excludes: exclude
                    .lines()
                    .map(str::trim)
                    .filter(|pattern| !pattern.is_empty())
                    .map(str::to_owned)
                    .collect(),
                includes: include
                    .lines()
                    .map(str::trim)
                    .filter(|pattern| !pattern.is_empty())
                    .map(str::to_owned)
                    .collect(),
                redact,
                fail_on_secret: fail_secret,
                dry_run,
                password: if password.is_empty() {
                    None
                } else {
                    Some(password.to_string())
                },
                password_policy,
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
                        let status = match event {
                            ProgressEvent::Scanning { examined, .. } => {
                                format!("Scanning… {examined} files")
                            }
                            ProgressEvent::Writing { included } => {
                                format!("Writing… {included} files")
                            }
                            ProgressEvent::Finished => "Finishing…".to_owned(),
                        };
                        let progress_ui = progress_ui.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = progress_ui.upgrade() {
                                w.set_status(status.into());
                            }
                        });
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
                        window.set_result_path(
                            result_path_for_outcome(&result_path, succeeded).into(),
                        );
                        window.set_status(status.into());
                    }
                });
            });
        },
    );
    window.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_user_selected_output_path_when_capability_changes() {
        assert_eq!(
            super::output_path_for_capability_change("/tmp/review.tar.gz", false, "zip"),
            "/tmp/review.tar.gz"
        );
    }

    use super::{result_path_for_outcome, settings_values_for_policy};
    use crate::security::PasswordPolicy;
    use std::path::Path;

    #[test]
    fn failed_run_clears_result_path() {
        assert_eq!(
            result_path_for_outcome(Path::new("bundle.tar.zst"), false),
            ""
        );
        assert_eq!(
            result_path_for_outcome(Path::new("bundle.tar.zst"), true),
            "bundle.tar.zst"
        );
    }

    #[test]
    fn settings_values_reflect_current_password_policy() {
        let values = settings_values_for_policy(&PasswordPolicy {
            minimum_length: 12,
            require_uppercase: false,
            require_lowercase: true,
            require_number: false,
            require_special: true,
        });
        assert_eq!(values, ("12".to_owned(), false, true, false, true));
    }
}
