// Offline browser regression against the generated synthetic vendor report.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');
(async () => {
  const report = path.resolve(process.argv[2] || 'examples/sanity/generated/vendor-report/report.html');
  const browser = await chromium.launch({channel:'msedge',headless:true});
  try {
    const page = await browser.newPage({acceptDownloads:true});
    const errors=[], requests=[];
    page.on('pageerror', e=>errors.push(e.message));
    page.on('request', r=>{if(/^https?:/.test(r.url()))requests.push(r.url());});
    await page.goto(pathToFileURL(report).href);
    const expected=await page.evaluate(()=>JSON.parse(document.getElementById('sanity-data').textContent));
    assert.equal(expected.validation_failed,false);
    for(const type of ['ATR','CDR','ATER','CTSR','CTRR'])assert(expected.records[type]>0,type);
    assert.equal(expected.units[0].previews.CTRR[0].setup_association,'resolved');
    assert.equal(expected.units[0].counts.CTRR,4);
    for(const viewport of [{width:1440,height:1000},{width:390,height:844}]) {
      await page.setViewportSize(viewport);
      const group=page.locator('#record-summary .record-group').filter({hasText:'ATR @'});
      const cell=group.locator('[data-status="not_checked"]').filter({hasText:'MOD_TIM'});
      assert.equal(await group.locator('[data-status="valid"], [data-status="invalid"], [data-status="missing"]').count(),0);
      await cell.hover();
      assert.equal(await page.locator('#entry-tooltip').innerText(),'2023-11-14 22:13:20 UTC');
      await cell.click();
      assert.equal(await page.evaluate(()=>document.activeElement.id),await cell.getAttribute('data-target'));
      assert.match(await page.locator('#metadata').innerText(),/CDR.*|CDR/s);
      assert.match(await page.locator('#metadata').innerText(),/AXES\[0\]\.TRACKING\[0\]\.TRACK_RNG_VAL/);
      await page.locator('#units tr').first().click();
      const activity=page.locator('#unit-summary [data-status="not_checked"]').filter({hasText:'ACTIVITY @'});
      await activity.hover();assert.equal(await page.locator('#entry-tooltip').innerText(),'Activity for unit 1, site 1');
      await activity.click();
      assert.equal(await page.evaluate(()=>document.activeElement.id),await activity.getAttribute('data-target'));
      await page.locator('#units tr').nth(1).click();
      await page.locator('#unit-summary [data-status="not_checked"]').filter({hasText:'ACTIVITY @'}).hover();
      assert.equal(await page.locator('#entry-tooltip').innerText(),'Activity for unit 2, site 2');
      assert.match(await page.locator('#preview').innerText(),/CTRR · showing 2\/4/);
      await page.locator('#units tr').nth(2).click();
      assert.match(await page.locator('#preview').innerText(),/CTRR · showing 2\/2/);
      await page.screenshot({path:path.resolve('target/vendor-'+viewport.width+'.png')});
    }
    const downloadPromise=page.waitForEvent('download');await page.locator('#export').click();
    const download=await downloadPromise;
    assert.deepEqual(JSON.parse(fs.readFileSync(await download.path(),'utf8')),expected);
    assert.deepEqual(errors,[]);assert.deepEqual(requests,[]);
    console.log('PASS: vendor records, Not checked legend/cells, UTC audit time, field jumps, site previews, setup references, JSON export, desktop/mobile and offline rendering');
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
