use std::fs::metadata;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[test]
fn release_scripts_are_executable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for name in ["release/build-artifacts.sh", "release/publish-release.sh"] {
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
