use std::fs::metadata;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const RELEASE_SCRIPTS: [&str; 2] = ["release/build-artifacts.sh", "release/publish-release.sh"];

#[test]
fn release_scripts_are_executable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for name in RELEASE_SCRIPTS {
        let path = root.join(name);
        let mode = metadata(&path)
            .unwrap_or_else(|e| panic!("{name} must exist: {e}"))
            .permissions()
            .mode();
        assert!(
            mode & 0o111 != 0,
            "{name} must be executable (mode {mode:o}); the release workflow invokes it directly"
        );
    }
}

#[test]
fn release_scripts_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for name in RELEASE_SCRIPTS {
        let status = Command::new("bash")
            .arg("-n")
            .arg(root.join(name))
            .status()
            .unwrap_or_else(|e| panic!("bash must be available to check {name}: {e}"));
        assert!(status.success(), "{name} must pass `bash -n`");
    }
}
