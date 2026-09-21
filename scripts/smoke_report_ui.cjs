// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
// Run after regenerating demo reports with the release CLI.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'..');
const output=path.join(root,'target/report-ui-screenshots');fs.mkdirSync(output,{recursive:true});
const cases=[
 ['dashboard','examples/ftr-patterns/generated/dashboard.html','dashboard-data'],
 ['dataset','examples/ftr-patterns/generated/dataset-dashboard.html','dashboard-data'],
 ['sanity','examples/sanity/generated/cp-report/report.html','sanity-data'],
 ['traceability','examples/traceability/generated/demo.html','trace-data'],
 ['ftr','examples/ftr-patterns/generated/report.html','fp-data']
];
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  for(const [kind,file,dataId] of cases){
   const context=await browser.newContext({viewport:{width:1500,height:1050},acceptDownloads:true});const page=await context.newPage(),errors=[],requests=[];
   page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/^https?:/.test(r.url()))requests.push(r.url())});
   await page.goto(pathToFileURL(path.join(root,file)).href+'?theme=mes&lang=en'+(['dashboard','dataset'].includes(kind)?'&tab=pareto':''));
   assert.equal(await page.locator('#report-theme option').count(),5);assert.equal(await page.locator('#report-language option').count(),5);
   const evidence=await page.locator('#'+dataId).textContent();
   if(kind==='dashboard'||kind==='dataset'){
    assert.equal(await page.locator('.tab').count(),6);
    assert.ok(await page.locator('#pareto').isVisible());
    await page.locator('[data-tab=pareto]').click();assert.equal(await page.locator('#pareto #latest-pareto').count(),1);
    await page.locator('#lp-level').selectOption('hard_bin');
    await page.locator('#lp-passing').check();
    await page.locator('#search').fill('HBIN');
    await page.locator('#pareto-table [data-sort="label"]').click();
    await page.locator('#pareto-table tbody button').first().click();
    await page.locator('#lp-close').click();
   }else if(kind==='sanity'){
    await page.locator('#units tr').first().click();await page.locator('#search').fill('UNIT1');
   }else if(kind==='traceability'){
    await page.locator('#wafer').fill('W1');await page.locator('#x').fill('1');
    await page.locator('#matrix tbody button').first().click();
   }
   for(const lang of ['zh','ja','ko','en','de']){
    await page.locator('#report-language').selectOption(lang);
    for(const theme of ['excel','mes','dark','quality','contrast']){
     await page.locator('#report-theme').selectOption(theme);
     assert.equal(await page.locator('html').getAttribute('data-theme'),theme);
     assert.equal(await page.locator('html').getAttribute('lang'),lang==='zh'?'zh-CN':lang);
     assert.equal(await page.locator('#'+dataId).textContent(),evidence);
     const misses=await page.evaluate(()=>[...document.querySelectorAll('[data-i18n]')].filter(n=>n.textContent!==ReportUI.t(n.dataset.i18n)).map(n=>n.dataset.i18n));assert.deepEqual(misses,[]);
     if(kind==='dashboard'||kind==='dataset'){
      assert.ok(await page.locator('#pareto').evaluate(n=>n.classList.contains('active')));
      assert.equal(await page.locator('#search').inputValue(),'HBIN');assert.equal(await page.locator('#pareto-table tbody tr').count(),1);
      assert.equal(await page.locator('#pareto-table tbody button').textContent(),'HBIN 1');
      assert.equal(await page.locator('#pareto-table th').first().getAttribute('aria-sort'),'ascending');
      assert.equal(await page.locator('#lp-passing').isChecked(),true);
     }else if(kind==='sanity'){
      assert.equal(await page.locator('#search').inputValue(),'UNIT1');assert.match(await page.locator('#unit-id').textContent(),/1/);
      assert.equal(await page.locator('#units tr.selected').count(),1);assert.match(await page.locator('#metadata').textContent(),/LOT001/);
     }else if(kind==='traceability'){
      assert.equal(await page.locator('#x').inputValue(),'1');assert.ok(await page.locator('#detail').isVisible());assert.ok(await page.locator('#attempts tbody tr').count()>0);
      assert.match(await page.locator('#detail-title').textContent(),/W1/);
     }
    }
   }
   // Check all Dashboard tabs and canvas labels after a language/theme switch.
   if(kind==='dashboard'||kind==='dataset'){
    for(const tab of ['overview','pareto','commonality','correlation','spatial','quality']){
     await page.locator('[data-tab='+tab+']').click();assert.ok(await page.locator('#'+tab).isVisible());
    }
    await page.locator('[data-tab=pareto]').click();await page.locator('#search').fill('');
   }
   await page.locator('#report-language').selectOption('zh');await page.locator('#report-theme').selectOption('dark');
   await page.screenshot({path:path.join(output,kind+'-dark-zh.png'),fullPage:true});
   await page.locator('#report-theme').selectOption('quality');
   await page.screenshot({path:path.join(output,kind+'-quality-zh.png'),fullPage:true});
   await page.reload();assert.equal(await page.locator('#report-language').inputValue(),'zh');assert.equal(await page.locator('#report-theme').inputValue(),'quality');
   await page.setViewportSize({width:390,height:844});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
   await page.screenshot({path:path.join(output,kind+'-mobile.png'),fullPage:true});
   // Preferences survive reload without a query where browser storage is available.
   await page.goto(pathToFileURL(path.join(root,file)).href);assert.equal(await page.locator('#report-theme').inputValue(),'quality');
   assert.deepEqual(errors,[],kind);assert.deepEqual(requests,[],kind);await context.close();
  }
  console.log('Report UI passed: 5 report routes × 5 languages × 5 themes, Pareto placement, state/evidence preservation, all Dashboard tabs, persistence, mobile, no network requests.');
 }finally{await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
