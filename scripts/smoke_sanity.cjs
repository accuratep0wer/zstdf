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
    const page = await browser.newPage({ acceptDownloads: true, timezoneId: 'America/New_York' });
    const errors = [], requests = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('request', r => { if (/^https?:/.test(r.url())) requests.push(r.url()); });
    for (const viewport of [{ width: 1440, height: 1000 }, { width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      await page.goto(pathToFileURL(report).href);
      assert.match(await page.locator('#status').innerText(), /checks passed/);
      assert.match(await page.locator('#metadata').innerText(), /LOT001/);
      assert.match(await page.locator('#record-summary').innerText(), /MIR @/);
      const valid = page.locator('#record-summary .status-entry[data-status="valid"]').filter({ hasText: 'LOT_ID' }).first();
      const missing = page.locator('#record-summary .status-entry[data-status="not_checked"]').first();
      assert(await missing.count());
      assert.equal(await page.locator('#record-summary .record-group').filter({hasText:/^(ATR|CDR|CTSR) @/}).count(),0);
      const mrr = page.locator('#record-summary .record-group').filter({hasText:'MRR @'});
      assert.equal(await mrr.locator('[data-status="valid"]').count(),1);
      assert.equal(await mrr.locator('[data-status="not_checked"]').count(),3);
      const csvPath = await page.locator('#checks').evaluate(a=>decodeURIComponent(new URL(a.href).pathname).replace(/^\/(?=[A-Za-z]:)/,''));
      const policy = await page.evaluate(()=>data.checks);
      assert.equal(fs.readFileSync(csvPath,'utf8'),policy.csv);
      assert.equal(require('node:crypto').createHash('sha256').update(policy.csv).digest('hex'),policy.hash);
      assert.equal(await valid.evaluate(n => getComputedStyle(n).backgroundColor), 'rgb(229, 244, 237)');
      assert.equal(await missing.evaluate(n => getComputedStyle(n).backgroundColor), 'rgb(239, 241, 243)');
      const nameBox = await valid.locator('.entry-name').boundingBox();
      const cellBox = await valid.boundingBox();
      assert(nameBox.x >= cellBox.x && nameBox.y >= cellBox.y);
      assert(nameBox.y + nameBox.height <= cellBox.y + cellBox.height);
      assert(cellBox.height <= 40, 'Single-line entries use compact spreadsheet cells');
      if(viewport.width === 390) {
        const rows = await page.locator('#record-summary .record-group').filter({ hasText: 'MIR @' }).locator('.status-entry').evaluateAll(ns => new Set(ns.map(n=>Math.round(n.getBoundingClientRect().top))).size);
        assert(rows > 1, 'Narrow cross-table entries wrap to multiple rows');
      }
      await valid.hover();
      assert.equal(await page.locator('#entry-tooltip').innerText(), 'LOT001');
      const targetId = await valid.getAttribute('data-target');
      await valid.click();
      assert.equal(await page.evaluate(() => document.activeElement.id), targetId);
      const target = page.locator('[id="' + targetId + '"]');
      assert.match(await target.getAttribute('class'), /entry-focus/);
      assert(await target.locator('pre').isVisible());
      await missing.focus();
      await page.keyboard.press('Enter');
      assert.equal(await page.evaluate(() => document.activeElement.id), await missing.getAttribute('data-target'));

      assert.equal(await page.locator('#units tr').count(), 3);
      await page.locator('#units tr').first().click();
      assert.match(await page.locator('#preview').innerText(), /PTR · Showing 2\/3/);
      assert.match(await page.locator('#preview').innerText(), /MPR · Showing 2\/3/);
      assert.match(await page.locator('#preview').innerText(), /1\.25/);
      const previewBlock = page.locator('#unit-summary .status-entry').filter({ hasText: 'RESULT @' }).first();
      assert(await previewBlock.count());
      await previewBlock.click();
      assert.equal(await page.evaluate(() => document.activeElement.id), await previewBlock.getAttribute('data-target'));
      await page.locator('#preview tr[data-record-type="PTR"]').first().locator('summary').click();

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
      assert.match(await page.locator('#preview').innerText(), /PTR · Showing 2\/3/);
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
    assert.deepEqual(await page.evaluate(() => {
      const f = {name:'START_T',kind:'U4',status:'valid'};
      return [timestampUtc(f,1700000000), timestampUtc(f,4294967295),
        timestampUtc(f,0), timestampUtc(f,-1), timestampUtc(f,1.5), timestampUtc(f,4294967296),
        timestampUtc({...f,status:'missing'},1700000000), timestampUtc({...f,name:'TEST_T'},1700000000)];
    }), ['2023-11-14 22:13:20 UTC','2106-02-07 06:28:15 UTC',null,null,null,null,null,null]);
    await page.evaluate(() => {
      const f = data.runs[0].metadata.find(m=>m.type==='MIR').fields.find(f=>f.name==='START_T');
      f.raw=1700000000; f.effective=1700000100; f.status='valid'; $('run').selectedIndex=0;run();
    });
    const timeBlock = page.locator('#record-summary .status-entry').filter({hasText:'START_T'}).first();
    await timeBlock.hover();
    assert.equal(await page.locator('#entry-tooltip').innerText(), '2023-11-14 22:13:20 UTC');
    await timeBlock.click();
    const timeRow = page.locator('[id="'+await timeBlock.getAttribute('data-target')+'"]');
    assert.match(await timeRow.innerText(), /1700000000/);
    assert.match(await timeRow.innerText(), /2023-11-14 22:13:20 UTC/);
    await page.evaluate(() => {
      const f = data.runs[0].metadata.find(m=>m.type==='MIR').fields.find(f=>f.name==='LOT_ID');
      f.raw='RAW ONLY'; f.effective='PROCESSED VALUE'; run();
    });
    const rawBlock=page.locator('#record-summary .status-entry').filter({hasText:'LOT_ID'}).first();
    await rawBlock.hover();
    assert.equal(await page.locator('#entry-tooltip').innerText(),'RAW ONLY');
    assert.equal(await rawBlock.getAttribute('title'),'RAW ONLY');
    await page.evaluate(() => {
      const f=data.runs[0].metadata.find(m=>m.type==='MIR').fields.find(f=>f.name==='LOT_ID');
      f.raw=null; f.effective='INHERITED VALUE'; run();
    });
    await rawBlock.hover();
    assert.equal(await page.locator('#entry-tooltip').innerText(),'[null]');
    assert.equal(await rawBlock.getAttribute('title'),'[null]');
    await rawBlock.focus();
    assert.equal(await page.locator('#entry-tooltip').innerText(),'[null]');

    await page.evaluate(() => {
      const f=data.runs[0].metadata.find(m=>m.type==='MIR').fields.find(f=>f.name==='LOT_ID');
      f.status='missing'; f.raw=null; f.effective=null; run();
    });
    const requiredMissing=page.locator('#record-summary [data-status="missing"]').filter({hasText:'LOT_ID'});
    assert.equal(await requiredMissing.evaluate(n=>getComputedStyle(n).backgroundColor),'rgb(239, 241, 243)');
    await requiredMissing.hover();assert.equal(await page.locator('#entry-tooltip').innerText(),'[null]');
    await page.evaluate(() => {
      const f = data.runs[0].metadata.find(m => m.type === 'MIR').fields.find(f => f.name === 'LOT_ID');
      f.status = 'invalid'; f.raw = f.effective = '<img src=x onerror=alert(1)>\\nBAD';
      $('run').selectedIndex = 0; run();
    });
    const invalid = page.locator('#record-summary .status-entry[data-status="invalid"]').filter({ hasText: 'LOT_ID' }).first();
    assert.equal(await invalid.evaluate(n => getComputedStyle(n).backgroundColor), 'rgb(255, 240, 233)');
    await invalid.hover();
    assert.match(await page.locator('#entry-tooltip').innerText(), /<img src=x/);
    assert.equal(await page.locator('#entry-tooltip img').count(), 0);
    await invalid.click();
    assert.equal(await page.evaluate(() => document.activeElement.id), await invalid.getAttribute('data-target'));
    // A semantically valid value that violates a product profile must not remain green.
    await page.evaluate(() => {
      const r = data.runs[0], m = r.metadata.find(m => m.type === 'MIR'), f = m.fields.find(f => f.name === 'LOT_ID');
      f.status = 'valid';
      profileIndex.set(JSON.stringify([r.source,m.offset,f.name]), [{rule:'profile',field:f.name,message:'Product mismatch'}]);
      run();
    });
    assert.equal(await page.locator('#record-summary .status-entry[data-status="invalid"]').filter({ hasText: 'LOT_ID' }).count(), 1);
    assert.deepEqual(errors, []);
    assert.deepEqual(requests, []);
    console.log('PASS: colored cells/wrapped labels/UTC dates/tooltips/keyboard/jumps/injection safety, run switching/fields, unit selection/filtering, first-two previews, JSON export, desktop/mobile, offline (' + runs + ' runs)');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
