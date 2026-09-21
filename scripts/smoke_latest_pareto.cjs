// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
// Run after examples/latest-pareto/generate_demo.py. Requires Playwright and Edge.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'../examples/latest-pareto/generated');
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage({viewport:{width:1536,height:1050},acceptDownloads:true}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(path.join(root,'dashboard.html')).href+'?tab=pareto&theme=quality&lang=en');
  const evidence=JSON.parse(await page.locator('#latest-pareto-data').textContent());
  assert.equal(evidence.attempts.length,15);assert.equal(evidence.attempts.filter(a=>a.is_latest).length,7);
  assert.equal(evidence.attempts.filter(a=>a.is_latest===false).length,8);
  assert.equal(evidence.attempts.filter(a=>a.is_latest&&a.passed).length,3);
  assert.ok(!(await page.locator('#lp-issues').isVisible()));
  const counts=async()=>Object.fromEntries(await page.locator('#pareto-table tbody tr').evaluateAll(rows=>rows.map(r=>[r.cells[0].innerText,Number(r.cells[1].innerText)])));
  assert.deepEqual(await counts(),{'HBIN 10':2,'HBIN 11':1,'HBIN 9':1});
  assert.equal(await page.locator('#lp-chart rect[data-mode]').count(),4);
  await page.locator('#lp-passing').check();assert.equal((await counts())['HBIN 1'],3);
  await page.locator('#lp-level').selectOption('soft_bin');assert.equal((await counts())['SBIN 10'],3);assert.equal((await counts())['SBIN 100'],2);
  await page.locator('#lp-passing').uncheck();assert.equal((await counts())['SBIN 10'],undefined);
  await page.locator('#lp-level').selectOption('tests');
  assert.equal(await page.locator('#pareto-table tbody tr').count(),6);
  await page.locator('#lp-level').selectOption('patterns');
  assert.deepEqual(await counts(),{'pattern/leak':2,'pattern/scan':1,'pattern/timing':1});
  assert.ok(!(await page.locator('#lp-passing-label').isVisible()));
  const stacked=await page.locator('#lp-chart rect[data-mode="0"]').evaluateAll(rs=>rs.map(r=>({x:r.getAttribute('x'),y:r.getAttribute('y')})));
  assert.equal(stacked.length,2);assert.equal(stacked[0].y,stacked[1].y);assert.notEqual(stacked[0].x,stacked[1].x);
  const colors=await page.locator('#lp-chart rect[data-mode="0"]').evaluateAll(rs=>rs.map(r=>r.getAttribute('fill')));
  assert.notEqual(colors[0],colors[1]);
  await page.locator('#lp-layout').selectOption('grouped');
  const grouped=await page.locator('#lp-chart rect[data-mode="0"]').evaluateAll(rs=>rs.map(r=>({x:r.getAttribute('x'),y:r.getAttribute('y')})));
  assert.equal(grouped[0].x,grouped[1].x);assert.notEqual(grouped[0].y,grouped[1].y);
  await page.locator('#lp-chart rect[data-mode="0"]').first().focus();await page.keyboard.press('Enter');
  assert.ok(await page.locator('#lp-detail').isVisible());const details=JSON.parse(await page.locator('#lp-detail-content').textContent());assert.equal(details.length,2);assert.ok(details.every(a=>a.is_latest===true&&a.mir_start_utc.endsWith('Z')));await page.locator('#lp-close').click();
  await page.locator('#lot-select').selectOption('1');assert.deepEqual(await counts(),{'pattern/leak':1,'pattern/timing':1});
  await page.locator('#lot-select').selectOption('0');await page.locator('#search').fill('pattern/leak');assert.deepEqual(await counts(),{'pattern/leak':2});
  for(const lang of ['en','zh','ja','ko','de'])for(const theme of ['excel','mes','dark','quality','contrast']){
   await page.locator('#report-language').selectOption(lang);await page.locator('#report-theme').selectOption(theme);
   assert.equal(await page.locator('#lp-level').inputValue(),'patterns');assert.equal(await page.locator('#lp-layout').inputValue(),'grouped');
   assert.deepEqual(await counts(),{'pattern/leak':2});
   const misses=await page.evaluate(()=>[...document.querySelectorAll('#latest-pareto [data-i18n]')].filter(n=>n.textContent!==ReportUI.t(n.dataset.i18n)).map(n=>n.dataset.i18n));assert.deepEqual(misses,[]);
  }
  await page.locator('#report-language').selectOption('en');await page.locator('#report-theme').selectOption('quality');await page.locator('#search').fill('');
  let d=page.waitForEvent('download');await page.locator('#lp-export').click();let file=await d;assert.deepEqual(JSON.parse(fs.readFileSync(await file.path(),'utf8')),evidence);
  d=page.waitForEvent('download');await page.locator('#lp-csv').click();file=await d;const csv=fs.readFileSync(await file.path(),'utf8');assert.match(csv,/"pattern\/leak","2","0","2","0"/);assert.equal(csv.split('\r\n').length,4);
  await page.locator('#lp-level').selectOption('hard_bin');await page.locator('#lp-layout').selectOption('stacked');await page.locator('#lp-passing').check();
  await page.screenshot({path:path.join(root,'pareto-stacked.png'),fullPage:true});
  await page.locator('#lp-layout').selectOption('grouped');await page.screenshot({path:path.join(root,'pareto-grouped.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.screenshot({path:path.join(root,'pareto-mobile.png'),fullPage:true});
  // All categories remain accessible, and exported counts are not page-limited.
  const expanded=structuredClone(evidence),base=evidence.attempts.find(a=>a.is_latest);
  expanded.attempts=Array.from({length:58},(_,i)=>({...base,ecid:'DEVICE_'+i,sequence:i+1,hard_bin:i+2,passed:false,patterns:[{key:'pattern'+i,label:i===0?'</script><img src=x onerror=alert(1)>':'pattern-'+i,passed:false}]}));
  const html=fs.readFileSync(path.join(root,'dashboard.html'),'utf8').replace(/(<script type="application\/json" id="latest-pareto-data">)[\s\S]*?(<\/script>)/,(_,a,b)=>a+JSON.stringify(expanded).replace(/</g,'\\u003c')+b);
  const expandedPath=path.join(root,'pagination.html');fs.writeFileSync(expandedPath,html);
  await page.goto(pathToFileURL(expandedPath).href+'?tab=pareto&lang=en');
  assert.equal(await page.locator('#pareto-table tbody tr').count(),25);await page.locator('#lp-next').click();assert.equal(await page.locator('#pareto-table tbody tr').count(),25);await page.locator('#lp-next').click();assert.equal(await page.locator('#pareto-table tbody tr').count(),8);
  d=page.waitForEvent('download');await page.locator('#lp-csv').click();file=await d;assert.equal(fs.readFileSync(await file.path(),'utf8').split('\r\n').length,59);
  await page.locator('#lp-level').selectOption('patterns');await page.locator('#search').fill('onerror');assert.equal(await page.locator('#pareto-table tbody tr').count(),1);assert.equal(await page.locator('#latest-pareto img').count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS: latest-device counts, all four levels, passing bins, stacked/grouped geometry, lot/search, drilldown, JSON/CSV, pagination, safe names, 25 style/language combinations, mobile.');
 }finally{await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
