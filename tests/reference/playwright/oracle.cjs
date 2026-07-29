'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');
const playwrightVersion = require('playwright/package.json').version;

const cases = [
  {
    control: 'radio',
    url: 'https://www.w3.org/WAI/ARIA/apg/patterns/radio/examples/radio/',
    role: 'radio', name: 'Deep dish', expectedName: 'Deep dish', state: 'aria-checked',
  },
  {
    control: 'switch',
    url: 'https://www.w3.org/WAI/ARIA/apg/patterns/switch/examples/switch/',
    role: 'switch', name: null, expectedName: 'Notifications', state: 'aria-checked',
  },
  {
    control: 'tab',
    url: 'https://www.w3.org/WAI/ARIA/apg/patterns/tabs/examples/tabs-manual/',
    role: 'tab', name: 'Carl Andersen', expectedName: 'Carl Andersen', state: 'aria-selected',
  },
  {
    control: 'menu_item',
    url: 'https://www.w3.org/WAI/ARIA/apg/patterns/menubar/examples/menubar-navigation/',
    role: 'menuitem', name: 'About', expectedName: 'About', state: 'aria-expanded',
  },
];

function argumentsFor(argv) {
  const values = new Map();
  for (let index = 2; index < argv.length; index += 2) values.set(argv[index], argv[index + 1]);
  const browser = values.get('--browser');
  const executablePath = values.get('--executable');
  const output = values.get('--output');
  if (!['chrome', 'edge'].includes(browser) || !executablePath || !output) {
    throw new Error('usage: oracle.cjs --browser chrome|edge --executable PATH --output DIR');
  }
  return { browser, executablePath, output };
}

async function runCase(browser, definition, output) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  try {
    await page.goto(definition.url, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(1000);
    const target = definition.name
      ? page.getByRole(definition.role, { name: definition.name, exact: true })
      : page.getByRole(definition.role).first();
    await target.waitFor({ state: 'visible' });
    const before = await target.getAttribute(definition.state);
    const ariaSnapshot = await target.ariaSnapshot();
    await target.click();
    await target.waitFor();
    const after = await target.getAttribute(definition.state);
    if (after !== 'true') throw new Error(`${definition.control} did not set ${definition.state}=true`);
    const screenshot = path.join(output, `${definition.control}.png`);
    await target.screenshot({ path: screenshot });
    return {
      control: definition.control,
      url: definition.url,
      role: definition.role,
      name: definition.expectedName,
      aria_snapshot: ariaSnapshot,
      state_attribute: definition.state,
      before,
      after,
      passed: true,
      screenshot: path.basename(screenshot),
    };
  } finally {
    await context.close();
  }
}

async function main() {
  const args = argumentsFor(process.argv);
  fs.mkdirSync(args.output, { recursive: true });
  const browser = await chromium.launch({ executablePath: args.executablePath, headless: true });
  let result;
  try {
    const results = [];
    for (const definition of cases) results.push(await runCase(browser, definition, args.output));
    result = {
      ok: true,
      mode: 'playwright_reference_oracle',
      browser: args.browser,
      playwright_version: playwrightVersion,
      browser_version: browser.version(),
      production_route: false,
      cases: results,
    };
  } catch (error) {
    result = {
      ok: false,
      mode: 'playwright_reference_oracle',
      browser: args.browser,
      playwright_version: playwrightVersion,
      production_route: false,
      error: String(error?.message || error),
    };
  } finally {
    await browser.close();
  }
  fs.writeFileSync(path.join(args.output, 'oracle.json'), `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify({ ok: result.ok, output: args.output })}\n`);
  if (!result.ok) process.exitCode = 1;
}

main().catch((error) => {
  process.stderr.write(`${String(error?.stack || error)}\n`);
  process.exitCode = 1;
});
