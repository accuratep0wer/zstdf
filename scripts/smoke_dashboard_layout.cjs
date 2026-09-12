// Run with local Edge and Playwright on NODE_PATH. Requires a nonempty EAV demo.
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');

(async () => {
  const report = process.argv[2];
  assert(report, 'Usage: node scripts/smoke_dashboard_layout.cjs dashboard.html');
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    for (const [width, height, dpr] of [[1440, 1000, 1], [390, 844, 1], [390, 844, 2]]) {
      const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: dpr });
      const page = await context.newPage(), errors = [], requests = [];
      page.on('pageerror', e => errors.push(e.message));
      page.on('request', r => { if (/^https?:/.test(r.url())) requests.push(r.url()); });
      // Capture real browser text metrics after transforms, not guessed label lengths.
      await page.addInitScript(() => {
        window.barLabels = [];
        const original = CanvasRenderingContext2D.prototype.fillText;
        CanvasRenderingContext2D.prototype.fillText = function (text, x, y, ...rest) {
          if (/^(site-yield|hard-bin|pareto|commonality|wafer-yield|quality)-chart$/.test(this.canvas.id)
              && /(%| parts| fails| hits| missing)$/.test(text)) {
            const m = this.measureText(text), t = this.getTransform();
            const left = x - (this.textAlign === 'right' ? m.width : this.textAlign === 'center' ? m.width / 2 : 0);
            window.barLabels.push({ text, left: left * t.a + t.e, right: (left + m.width) * t.a + t.e, width: this.canvas.width });
          }
          return original.call(this, text, x, y, ...rest);
        };
      });
      await page.goto(pathToFileURL(path.resolve(report)).href);
      for (const name of ['Overview', 'Pareto', 'Commonality', 'Correlation', 'Spatial', 'Quality']) {
        await page.getByRole('button', { name, exact: true }).click();
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 2), false, name + ' overflows viewport');
      }
      const labels = await page.evaluate(() => window.barLabels);
      assert(labels.some(v => v.text.endsWith('%')), 'No percentage label checked');
      for (const label of labels) assert(label.left >= -1 && label.right <= label.width + 1, JSON.stringify(label));
      await page.getByRole('button', { name: 'Pareto', exact: true }).click();
      await page.locator('#pareto-chart').click({ position: { x: 145, y: 30 } });
      assert.match(await page.locator('#test-detail').innerText(), /Fail \d/);
      await page.locator('#search').fill('__no_such_test__');
      assert.match(await page.locator('#pareto-table').innerText(), /No data/);
      assert.deepEqual(errors, []);
      assert.deepEqual(requests, []);
      await context.close();
    }
    console.log('PASS: bar text bounds, all tabs, desktop/mobile/DPR2, bar picking, empty filters, zero script errors');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
