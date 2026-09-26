#!/usr/bin/env bash
# Starts the unified binary in web mode for the Playwright suite, with a local
# repository fixture and an allowed local root.
#
# Both paths are created with mktemp so a pre-created symlink in a world
# writable directory can never redirect the cleanup or the fixture.
set -euo pipefail
cd "$(dirname "$0")/.."

port=${1:-8877}
root=$(mktemp -d "${TMPDIR:-/tmp}/rrs-pw-root.XXXXXX")
repo=$(mktemp -d "${TMPDIR:-/tmp}/rrs-pw-repo.XXXXXX")

cleanup() {
    # Only ever remove the directories this script created.
    case "$root" in */rrs-pw-root.*) rm -rf -- "$root" ;; esac
    case "$repo" in */rrs-pw-repo.*) rm -rf -- "$repo" ;; esac
}
trap cleanup EXIT INT TERM

cargo build --all-features --bin Rustrepo-sanitizer >/dev/null

(
    cd "$repo"
    git init -q
    git config user.email test@example.com
    git config user.name Test
    printf 'password: local-secret-value\n' > config.txt
    printf '# fixture\n' > README.md
    git add .
    git commit -q -m init
)

# The spec needs to know the fixture path.
repo_file=${RRS_TEST_REPO_FILE:-}
if [ -n "$repo_file" ]; then
    mkdir -p "$(dirname "$repo_file")"
    printf '%s\n' "$repo" > "$repo_file"
fi

RUSTREPO_WEB_BIND="127.0.0.1:$port" \
RUSTREPO_WEB_ROOT="$root" \
RUSTREPO_WEB_LOCAL_ROOTS="$repo" \
exec ./target/debug/Rustrepo-sanitizer --web
