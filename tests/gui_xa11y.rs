//! Native semantic smoke test. Run with a graphical session and AT-SPI:
//! `cargo test --test gui_xa11y -- --ignored --nocapture`.
#[test]
#[ignore = "requires a running native GUI, D-Bus, and AT-SPI"]
fn discovers_slint_controls_semantically() {
    use xa11y::{App, AppExt};
    let apps = App::list().expect("AT-SPI application list must be readable");
    eprintln!(
        "AT-SPI applications: {:?}",
        apps.iter().map(|app| &app.name).collect::<Vec<_>>()
    );
    let app = apps
        .into_iter()
        .find(|app| app.name.to_ascii_lowercase().contains("rustrepo"))
        .expect("Slint application must be discoverable through AT-SPI");
    let tree = app.dump(Some(4)).expect("AT-SPI tree must be readable");
    assert!(tree.contains("Rustrepo Sanitizer"));
    app.locator(r##"text_field[name="Repository path"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("repository input must be semantically discoverable");
    app.locator(r##"button[name="Browse for repository"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Browse control must be semantically discoverable");
    app.locator(r##"text_field[name="Output file path"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Output File input must be semantically discoverable");
    app.locator(r##"button[name="Browse for output folder"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Output Browse control must be semantically discoverable");
    app.locator(r##"combo_box[name="Archive format"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Archive selector must be semantically discoverable");
    app.locator(r##"combo_box[name="Compression"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Compression selector must be semantically discoverable");
    assert!(
        tree.contains("Help"),
        "desktop Help menu must be represented in the accessibility tree"
    );
    assert!(
        tree.contains("Settings"),
        "desktop Settings menu must be represented in the accessibility tree"
    );
    assert!(
        tree.contains("About"),
        "desktop About menu must be represented in the accessibility tree"
    );
    app.locator(r##"button[name="Cancel sanitization"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Cancel control must be semantically discoverable");
    app.locator(r##"check_box[name="Advanced options"]"##)
        .press()
        .expect("Advanced options must be semantically activatable");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let include_glob = app.locator(r##"text_field[name="Custom include glob"]"##);
    include_glob
        .scroll_into_view()
        .expect("Include Glob editor must be scrollable into view");
    include_glob
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Include Glob editor must be semantically discoverable");
    app.locator(r##"button[name="Add"]"##)
        .wait_visible(std::time::Duration::from_secs(5))
        .expect("Glob Add controls must be semantically discoverable");
    app.locator(r##"check_box[name="Advanced options"]"##)
        .press()
        .expect("Advanced options must be closable semantically");
    app.locator(r##"button[name="Sanitize repository"]"##)
        .press()
        .expect("sanitize must be semantically activatable");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let after = app.dump(Some(4)).expect("AT-SPI tree must remain readable");
    assert!(
        after.contains("Sanitizing") || after.contains("Complete") || after.contains("Error"),
        "sanitization action must update the accessible status: {after}"
    );
}
