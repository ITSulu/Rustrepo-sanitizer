// End-to-end browser coverage for the web UI.
//
// Runs against a live `Rustrepo-sanitizer --web` instance started by the test
// harness (see playwright.config.js). Public repositories are exercised for
// real acquisition; credentials and private operations are mocked.
const { test, expect } = require('@playwright/test');

// The fixture repository created by scripts/playwright-server.sh.
const FIXTURE_REPO = process.env.RRS_TEST_REPO || '/tmp/rrs-pw-repo';

const TERMINAL = '.status[data-kind="ok"], .status[data-kind="error"]';

/** Submits the form and waits for the job to reach a terminal state.
 *
 * The job page renders the state at request time, so progress is observed by
 * following the "Refresh status" link rather than by any auto-refresh.
 */
async function submitAndWaitForJob(page, { timeout = 120000 } = {}) {
  await Promise.all([
    page.waitForNavigation({ waitUntil: 'domcontentloaded' }),
    page.locator('button[type=submit]').click(),
  ]);
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await page.locator(TERMINAL).count()) {
      return page.locator(TERMINAL).first();
    }
    const refresh = page.getByRole('link', { name: 'Refresh status' });
    if (await refresh.count()) {
      await Promise.all([
        page.waitForNavigation({ waitUntil: 'domcontentloaded' }),
        refresh.click(),
      ]);
    }
    await page.waitForTimeout(500);
  }
  throw new Error(`job did not settle within ${timeout}ms`);
}

test.describe('web UI', () => {
  test('renders the sanitize and option reference navigation', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('nav#nav a[href="#sanitize"]')).toHaveText('Sanitize');
    await expect(page.locator('nav#nav a[href="#option-reference"]')).toHaveText('Option Reference');
    await expect(page.locator('#sanitize')).toBeVisible();
    await expect(page.locator('#option-reference')).toBeVisible();
  });

  test('navigation moves between sections', async ({ page }) => {
    await page.goto('/');
    await page.locator('nav#nav a[href="#option-reference"]').click();
    await expect(page).toHaveURL(/#option-reference$/);
  });

  test('exposes title-cased headings and labels', async ({ page }) => {
    await page.goto('/');
    for (const text of [
      'Repository Source',
      'Server Local Path',
      'Git URL',
      'Forgejo Repository',
      'GitHub Repository',
      'Branch Or Tag',
      'Maximum File Size',
      'Include Globs',
      'Exclude Globs',
      'Supported Formats',
      'Option Reference',
    ]) {
      // Match the visible label, not a hidden placeholder of the same text.
      await expect(
        page.locator('label, legend, h1, h2, h3').filter({ hasText: new RegExp(`^${text}$`) }).first(),
      ).toBeVisible();
    }
  });

  test('repository source layout orders Forgejo below the local path with a branch field', async ({ page }) => {
    await page.goto('/');
    const path = await page.locator('#path').boundingBox();
    const forgejo = await page.locator('#forgejo-repo').boundingBox();
    const branch = await page.locator('#git-ref').boundingBox();
    expect(forgejo.y).toBeGreaterThan(path.y);
    // The branch field sits to the right of the Forgejo field.
    expect(branch.x).toBeGreaterThan(forgejo.x);
  });

  test('repository and tag fields do not offer email autofill', async ({ page }) => {
    await page.goto('/');
    for (const field of ['#path', '#url', '#forgejo-repo', '#github-repo', '#git-ref']) {
      await expect(page.locator(field)).toHaveAttribute('autocomplete', 'off');
      await expect(page.locator(field)).not.toHaveAttribute('type', 'email');
    }
  });

  test('unselected dropdown options stay readable', async ({ page }) => {
    await page.goto('/');
    const option = page.locator('#format option').nth(1);
    const color = await option.evaluate((el) => getComputedStyle(el).color);
    const background = await option.evaluate((el) => getComputedStyle(el).backgroundColor);
    // A readable option must not inherit the dark card on dark text.
    expect(color).not.toBe(background);
  });

  test('maximum file size converts when the unit changes', async ({ page }) => {
    await page.goto('/');
    const bytes = page.locator('#max_file_size_bytes');
    await expect(bytes).toHaveValue('10485760'); // 10 MiB default

    await page.locator('#max_file_size_unit').selectOption('KiB');
    await expect(page.locator('#max_file_size')).toHaveValue('10240');
    await expect(bytes).toHaveValue('10485760'); // byte-equivalent size preserved

    await page.locator('#max_file_size_unit').selectOption('GiB');
    await expect(page.locator('#max_file_size')).toHaveValue('0.009765625');
    await expect(bytes).toHaveValue('10485760');
  });

  test('maximum file size accepts a typed value with its unit', async ({ page }) => {
    await page.goto('/');
    await page.locator('#max_file_size').fill('2');
    await page.locator('#max_file_size_unit').selectOption('MiB');
    await expect(page.locator('#max_file_size_bytes')).toHaveValue('2097152');
  });

  test('supported formats appear inside the output section below the size field', async ({ page }) => {
    await page.goto('/');
    const output = page.locator('fieldset', { hasText: 'Output' });
    await expect(output.getByText('Supported Formats')).toBeVisible();
    const order = await output.evaluate((el) => {
      const text = el.innerText;
      return {
        size: text.indexOf('Maximum File Size'),
        formats: text.indexOf('Supported Formats'),
      };
    });
    expect(order.size).toBeGreaterThan(-1);
    expect(order.formats).toBeGreaterThan(order.size);
  });

  test('option reference tooltips appear only after a long hover', async ({ page }) => {
    await page.goto('/');
    const field = page.locator('#option-reference .tip').first();
    const tip = page.locator('#tip-repository');
    // The dwell time gates the opacity transition, so opacity is the signal.
    const opacity = () =>
      tip.evaluate((el) => Number(getComputedStyle(el).opacity));

    expect(await opacity()).toBe(0);

    // A brief pass is not enough to reveal the help text.
    await field.hover();
    await page.waitForTimeout(1000);
    expect(await opacity()).toBeLessThan(0.5);

    // The full dwell reveals exactly one line of help.
    await expect
      .poll(() => opacity(), { timeout: 5000 })
      .toBeGreaterThan(0.9);
    await expect(tip).toHaveText(/Git repository to sanitize/);

    // Moving away hides it again promptly.
    await page.mouse.move(700, 3);
    await expect.poll(() => opacity(), { timeout: 3000 }).toBe(0);
  });

  test('every option reference field carries a one-line tooltip', async ({ page }) => {
    await page.goto('/');
    const tips = page.locator('#option-reference .tip-text');
    const count = await tips.count();
    expect(count).toBeGreaterThan(10);
    for (let i = 0; i < count; i += 1) {
      const tip = tips.nth(i);
      await expect(tip).toHaveAttribute('role', 'tooltip');
      const text = (await tip.textContent()).trim();
      expect(text.length).toBeGreaterThan(10);
      // One line of help, not a paragraph.
      expect(text.split('\n').length).toBeLessThanOrEqual(2);
    }
  });

  test('filters offer common globs and an add button', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('#include-add')).toBeVisible();
    await expect(page.locator('#exclude-add')).toBeVisible();
    await page.locator('#include-choice').selectOption('src/**/*.rs');
    await page.locator('#include-entry').fill('custom/**');
    await expect(page.locator('#include-entry')).toHaveValue('custom/**');
  });

  test('validation errors are announced and keep the entered values', async ({ page }) => {
    await page.goto('/');
    await page.locator('#mode').selectOption('forgejo');
    await page.locator('#forgejo-repo').fill('../etc');
    await page.locator('button[type=submit]').click();
    const alert = page.locator('#form-error');
    await expect(alert).toBeVisible();
    await expect(alert).toHaveAttribute('role', 'alert');
    await expect(page.locator('#forgejo-repo')).toHaveValue('../etc');
  });

  test('sanitizes a local repository and offers a download', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_LOCAL, 'local repository fixture not provided');
    await page.goto('/');
    await page.locator('#mode').selectOption('local_path');
    await page.locator('#path').fill(FIXTURE_REPO);
    const status = await submitAndWaitForJob(page);
    await expect(status).toContainText('Completed');
    await expect(page.getByRole('link', { name: /Download sanitized archive/i })).toBeVisible();
  });

  test('downloads the sanitized archive produced by a job', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_LOCAL, 'local repository fixture not provided');
    await page.goto('/');
    await page.locator('#mode').selectOption('local_path');
    await page.locator('#path').fill(FIXTURE_REPO);
    await submitAndWaitForJob(page);
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      page.getByRole('link', { name: /Download sanitized archive/i }).click(),
    ]);
    expect(download.suggestedFilename()).toMatch(/\.tar(\.\w+)?$/);
  });

  test('excludes files that do not match the include globs', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_LOCAL, 'local repository fixture not provided');
    await page.goto('/');
    await page.locator('#mode').selectOption('local_path');
    await page.locator('#path').fill(FIXTURE_REPO);
    await page.locator('#include-entry').fill('*.md');
    const status = await submitAndWaitForJob(page);
    // Only README.md matches, so the report records one included file.
    await expect(status).toContainText('1 files');
  });

  test('acquires a public GitHub repository and sanitizes it', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_NETWORK, 'network tests disabled');
    await page.goto('/');
    await page.locator('#mode').selectOption('git_url');
    await page.locator('#url').fill('https://github.com/ITSulu/Rustrepo-sanitizer');
    const status = await submitAndWaitForJob(page, { timeout: 300000 });
    await expect(status).toContainText('Completed');
    await expect(status).toContainText('files');
  });

  test('acquires a public repository at a named branch', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_NETWORK, 'network tests disabled');
    await page.goto('/');
    // git.itsulu.com resolves to a private LAN address, which the web server
    // rejects by design, so the public mirror exercises the branch field.
    await page.locator('#mode').selectOption('git_url');
    await page.locator('#url').fill('https://github.com/ITSulu/Rustrepo-sanitizer.git');
    await page.locator('#git-ref').fill('main');
    const status = await submitAndWaitForJob(page, { timeout: 300000 });
    await expect(status).toContainText('Completed');
    await expect(status).toContainText('files');
  });

  test('rejects the private Forgejo host address by design', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_NETWORK, 'network tests disabled');
    await page.goto('/');
    await page.locator('#mode').selectOption('git_url');
    await page.locator('#url').fill('https://git.itsulu.com/itsulu/Rustrepo-sanitizer.git');
    const status = await submitAndWaitForJob(page, { timeout: 120000 });
    await expect(status).toContainText('private, loopback, or reserved');
  });

  test('reports that the Forgejo integration needs server credentials', async ({ page }) => {
    await page.goto('/');
    await page.locator('#mode').selectOption('forgejo');
    await page.locator('#forgejo-repo').fill('itsulu/Rustrepo-sanitizer');
    const status = await submitAndWaitForJob(page);
    // Credentials are never mocked into the browser, so the server refuses.
    await expect(status).toContainText('not configured on the server');
  });

  test('rejects an unknown branch rather than silently using the default', async ({ page }) => {
    test.skip(!process.env.RRS_TEST_NETWORK, 'network tests disabled');
    await page.goto('/');
    await page.locator('#mode').selectOption('git_url');
    await page.locator('#url').fill('https://github.com/ITSulu/Rustrepo-sanitizer.git');
    await page.locator('#git-ref').fill('no-such-branch-2f9c1a');
    const status = await submitAndWaitForJob(page, { timeout: 300000 });
    await expect(status).toContainText('Failed');
  });

  test('rejects a private or loopback repository URL', async ({ page }) => {
    await page.goto('/');
    await page.locator('#mode').selectOption('git_url');
    await page.locator('#url').fill('https://127.0.0.1/secret.git');
    await page.locator('button[type=submit]').click();
    // The job is created and fails; the reason is surfaced on the job page.
    await expect(page.locator('.status[data-kind="error"]')).toContainText(
      /private, loopback, or reserved/i,
      { timeout: 60000 },
    );
  });

  test('is keyboard navigable with a visible focus indicator', async ({ page }) => {
    await page.goto('/');
    await page.keyboard.press('Tab');
    const skip = await page.evaluate(() => document.activeElement?.textContent?.trim());
    expect(skip).toContain('Skip to main content');
    const outline = await page.evaluate(() => {
      const el = document.querySelector('nav#nav a:focus') || document.activeElement;
      return el ? getComputedStyle(el).outlineStyle : '';
    });
    expect(outline).not.toBe('');
  });
});
