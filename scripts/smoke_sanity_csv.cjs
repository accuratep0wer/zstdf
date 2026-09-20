// Run after the vendor-checked-report example in docs/vendor-records.md.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL, fileURLToPath } = require('node:url');
const { createHash } = require('node:crypto');
const { chromium } = require('playwright');
(async () => {
  const report = path.resolve(process.argv[2] || 'examples/sanity/generated/vendor-checked-report/report.html');
  const browser = await chromium.launch({channel:'msedge', headless:true});
  try {
    const page = await browser.newPage({acceptDownloads:true});
    const errors=[];
    page.on('pageerror', e=>errors.push(e.message));
    for (const viewport of [{width:1440,height:1000},{width:390,height:844}]) {
      await page.setViewportSize(viewport);
      await page.goto(pathToFileURL(report).href);
      const data=await page.evaluate(()=>JSON.parse(document.getElementById('sanity-data').textContent));
      assert.equal(data.validation_failed,false);
      for (const [type,name] of [['ATR','MOD_TIM'],['CDR','CHN_NAM'],['CTSR','CHAR_NAM']]) {
        const group=page.locator('#record-summary .record-group').filter({hasText:new RegExp('^'+type+' @')}).first();
        const cell=group.locator('[data-status="valid"] .entry-name').filter({hasText:new RegExp('^'+name+'$')}).locator('..');
        assert.equal(await cell.count(),1,type+'.'+name);
        await cell.hover();
        assert((await page.locator('#entry-tooltip').innerText()).length>0);
        await cell.click();
        assert.equal(await page.evaluate(()=>document.activeElement.id),await cell.getAttribute('data-target'));
      }
      const mrr=page.locator('#record-summary .record-group').filter({hasText:'MRR @'});
      assert.equal(await mrr.locator('[data-status="valid"]').count(),1);
      assert.equal(await mrr.locator('[data-status="not_checked"]').count(),3);
      const csvUrl=await page.locator('#checks').getAttribute('href');
      const csvPath=fileURLToPath(new URL(csvUrl,pathToFileURL(report)));
      assert.equal(fs.readFileSync(csvPath,'utf8'),data.checks.csv);
      const manifestUrl=await page.locator('#manifest').getAttribute('href');
      const manifest=JSON.parse(fs.readFileSync(fileURLToPath(new URL(manifestUrl,pathToFileURL(report))),'utf8'));
      assert.equal(manifest.checks_hash,createHash('sha256').update(data.checks.csv).digest('hex'));
      assert.equal(manifest.profile_hash,data.profile_hash);
      assert.equal(manifest.files['checks.csv'].sha256,manifest.checks_hash);
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+2),false);
    }
    await page.screenshot({path:'target/csv-vendor-checked.png'});
    // The generated inner report also resolves the CSV within its own bundle.
    const href=await page.locator('#checks').getAttribute('href');
    const inner=new URL(href,pathToFileURL(report));inner.pathname=inner.pathname.replace(/checks\.csv$/,'report.html');
    await page.goto(inner.href);
    assert.equal(await page.locator('#checks').getAttribute('href'),'checks.csv');
    assert.deepEqual(errors,[]);
    console.log('PASS: CSV opt-in fields, field jumps, MRR selection, policy download/hash/manifest, inner report links, desktop/mobile');
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exitCode=1;});
