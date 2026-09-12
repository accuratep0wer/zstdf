// Use the installed Edge and Playwright on NODE_PATH; operates on file:// offline.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');
(async () => {
  const report = path.resolve(process.argv[2] || 'examples/sanity/generated/cp-report/report.html');
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage({ acceptDownloads: true });
    const errors = [], requests = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('request', r => { if (/^https?:/.test(r.url())) requests.push(r.url()); });
    for (const viewport of [{ width: 1440, height: 1000 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await page.goto(pathToFileURL(report).href);
      assert.match(await page.locator('#status').innerText(), /checks passed/);
      assert.match(await page.locator('#metadata').innerText(), /LOT001/);
      assert.equal(await page.locator('#units tr').count(), 3);
      await page.locator('#units tr').first().click();
      assert.match(await page.locator('#preview').innerText(), /PTR · showing 2\/3/);
      assert.match(await page.locator('#preview').innerText(), /MPR · showing 2\/3/);
      assert.match(await page.locator('#preview').innerText(), /1\.25/);
      const ptr = page.locator('#preview tr[data-record-type="PTR"]').first();
      assert.equal(await ptr.locator('.measurement-value').innerText(), '1.25');
      assert.equal(await ptr.locator('pre').isVisible(), false);
      await ptr.locator('summary').click();
      assert.match(await ptr.locator('pre').innerText(), /3fa00000/);
      await ptr.locator('summary').click();
      assert.equal(await page.locator('#preview tr[data-record-type="GDR"] .measurement-value').first().innerText(), '1.0000000000000002');
      assert.equal(await page.locator('#preview tr[data-record-type="FTR"] .measurement-value').first().innerText(), 'PASS');
      assert.match(await page.locator('#run option').first().innerText(), /CP.*demo-cp-v1/);
      await page.locator('#search').fill('UNIT2');
      assert.equal(await page.locator('#units tr').count(), 1);
      await page.locator('#units tr').click();
      assert.match(await page.locator('#unit-id').innerText(), /Unit 2/);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 2), false);
    }
    const runs = await page.locator('#run option').count();
    for (let i = 0; i < runs; i++) {
      await page.selectOption('#run', { index: i });
      await page.locator('#search').fill('');
      assert.equal(await page.locator('#units tr').count(), 3);
      assert.match(await page.locator('#metadata').innerText(), /LOT001/);
      await page.locator('#units tr').first().click();
      assert.match(await page.locator('#preview').innerText(), /PTR · showing 2\/3/);
    }
    await page.locator('#search').fill('UNIT2');
    const pending = page.waitForEvent('download');
    await page.click('#export');
    const download = await pending;
    const data = JSON.parse(fs.readFileSync(await download.path(), 'utf8'));
    assert.equal(data.units.length, 3 * runs); // Export includes other runs and hidden units.
    if (runs > 1) {
      assert.deepEqual([...new Set(data.runs.map(r => r.domain))].sort(), ['cp', 'ft']);
      assert(data.runs.every(r => r.profile_id && /^[a-f0-9]{64}$/.test(r.profile_hash)));
    }
    assert.equal(data.schema, 'sanity-v1');
    const embedded = JSON.parse(fs.readFileSync(report, 'utf8').split('id="sanity-data" type="application/json">')[1].split('</script>')[0]);
    assert.deepEqual(data, embedded); // Display formatting must not alter evidence or precision.
    assert.deepEqual(errors, []);
    assert.deepEqual(requests, []);
    console.log('PASS: run switching/fields, unit selection/filtering, first-two previews, JSON export, desktop/mobile, offline (' + runs + ' runs)');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
