const { defineConfig } = require('@playwright/test');
const os = require('os');
const path = require('path');

const port = Number(process.env.RRS_TEST_PORT || 8877);
const baseURL = `http://127.0.0.1:${port}`;

// The harness creates the fixture with mktemp and records its path here, so the
// spec never has to guess a shared temporary directory.
const repoPathFile = process.env.RRS_TEST_REPO_FILE
  || path.join(os.tmpdir(), `rrs-pw-fixture-${port}.path`);

module.exports = defineConfig({
  testDir: './tests/playwright',
  timeout: 180000,
  expect: { timeout: 15000 },
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL,
  },
  webServer: {
    command: `RRS_TEST_REPO_FILE=${repoPathFile} ./scripts/playwright-server.sh ${port}`,
    url: `${baseURL}/api/health`,
    // Always start a fresh server so the fixture and allowed roots are never stale.
    reuseExistingServer: false,
    timeout: 120000,
  },
  // Runs after the server is ready so the recorded fixture path exists.
  globalSetup: require.resolve('./tests/playwright/global-setup.js'),
});
