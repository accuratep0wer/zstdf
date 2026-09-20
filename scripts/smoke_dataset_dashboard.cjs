// Run with Playwright available through NODE_PATH and a locally installed Edge.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { execFileSync } = require('node:child_process');
const { chromium } = require('playwright');

function fixture(lot, passed) {
  const records = [];
  function record(type, sub, body) {
    body = Buffer.from(body);
    const header = Buffer.alloc(4); header.writeUInt16LE(body.length); header[2] = type; header[3] = sub;
    records.push(header, body);
  }
  record(0, 10, [2, 4]);
  record(1, 10, Buffer.concat([Buffer.alloc(15), Buffer.from([lot.length]), Buffer.from(lot), Buffer.alloc(4)]));
  record(5, 10, [1, 0]);
  for (let test = 1; test <= 2; test++) {
    const ptr = Buffer.alloc(12); ptr.writeUInt32LE(test); ptr[4] = 1; ptr[6] = passed ? 0 : 128; ptr.writeFloatLE(test, 8);
    record(15, 10, ptr);
  }
  record(5, 20, [1, 0, passed ? 0 : 8, 2, 0, passed ? 1 : 9, 0, 1, 0]);
  return Buffer.concat(records);
}

(async () => {
  const cli = path.resolve(process.argv[2] || 'target/debug/zstdf-cli.exe');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zstdf-dashboard-smoke-'));
  let browser;
  try {
    const first = path.join(root, 'one.stdf'), second = path.join(root, 'two.stdf');
    fs.writeFileSync(first, fixture('LOT1', true)); fs.writeFileSync(second, fixture('LOT2', false));
    const dataset = path.join(root, 'dataset'), output = path.join(root, 'dashboard.html');
    execFileSync(cli, ['convert', '--layout', 'catalog', '--output-dir', dataset, '--row-group-rows', '1', first, second]);
    execFileSync(cli, ['verify-dataset', dataset]);
    execFileSync(cli, ['dashboard-dir', dataset, output]);
    browser = await chromium.launch({ channel: 'msedge', headless: true });
    const page = await browser.newPage(); const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport); await page.goto(pathToFileURL(output).href);
      assert.equal(await page.locator('#lot-select option').count(), 3);
      assert.match(await page.locator('#kpis').innerText(), /50\.00%/);
      await page.selectOption('#lot-select', '1');
      assert.match(await page.locator('#kpis').innerText(), /100\.00%/);
      await page.selectOption('#lot-select', '2');
      assert.match(await page.locator('#kpis').innerText(), /0\.00%/);
      await page.click('[data-tab="pareto"]');
      assert.ok(await page.locator('#pareto').isVisible());
      await page.locator('#search').fill('nonexistent');
      assert.match(await page.locator('#pareto-table').innerText(), /No data for current filters/);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 2), false);
    }
    assert.deepEqual(errors, []);
    console.log('PASS: desktop/mobile rendering, lot yields, tabs, search, and no JavaScript errors.');
  } finally {
    if (browser) await browser.close();
    const resolved = path.resolve(root);
    assert.equal(path.dirname(resolved), path.resolve(os.tmpdir()));
    assert.ok(path.basename(resolved).startsWith('zstdf-dashboard-smoke-'));
    fs.rmSync(resolved, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
