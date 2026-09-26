// Publishes the fixture repository path created by scripts/playwright-server.sh
// so the spec can reference it without a hard-coded temporary directory.
const fs = require('fs');
const os = require('os');
const path = require('path');

module.exports = async () => {
  const file =
    process.env.RRS_TEST_REPO_FILE
    || path.join(os.tmpdir(), `rrs-pw-fixture-${process.env.RRS_TEST_PORT || 8877}.path`);
  if (!fs.existsSync(file)) {
    throw new Error(`fixture path file not found: ${file}`);
  }
  const repo = fs.readFileSync(file, 'utf8').trim();
  if (!repo.startsWith('/')) {
    throw new Error(`fixture path is not absolute: ${repo}`);
  }
  process.env.RRS_TEST_REPO = repo;
};
