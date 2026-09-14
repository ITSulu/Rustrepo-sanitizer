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
    app.locator(r##"check_box[name="Advanced options"]"##)
        .press()
        .expect("Advanced options must be semantically activatable");
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
