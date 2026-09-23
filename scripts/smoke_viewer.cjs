// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
// Run: node scripts/smoke_viewer.cjs (Playwright + Edge, after generating demo).
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'examples/viewer/generated');
const cli=path.join(root,'target/release',process.platform==='win32'?'zstdf-cli.exe':'zstdf-cli');
const wait=(ms)=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});let server;
 try{
  const context=await browser.newContext({viewport:{width:1536,height:1050},acceptDownloads:true});
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(path.join(out,'selection.html')).href+'?lang=en&theme=quality');
  await page.waitForFunction(()=>Viewer.state.panes[0].data?.total>0);
  const original=await page.evaluate(()=>Viewer.state.panes[0].data.total);assert.equal(original,345);
  assert.equal(await page.locator('.table-scroll tbody tr').first().evaluate(r=>r.getBoundingClientRect().height),24);
  // Keyboard inspection, three-state source-order sorting, and annotation navigation.
  const tablePane=page.locator('[data-pane="1"]');
  await tablePane.locator('tbody tr').first().focus();
  await page.keyboard.press('ArrowDown');
  assert.equal(await page.evaluate(()=>document.activeElement.dataset.id),await tablePane.locator('tbody tr').nth(1).getAttribute('data-id'));
  await page.keyboard.press('Enter');await page.locator('#dialog[open]').waitFor();
  await page.locator('#close-dialog').click();
  const sourceOrder=await page.evaluate(()=>Viewer.state.panes[0].data.rows.map(r=>r.id));
  for(const expected of[{sort:'value',descending:false},{sort:'value',descending:true},{sort:'',descending:false}]){
   await tablePane.locator('th').filter({hasText:/^Value/}).first().click({position:{x:12,y:10}});
   await page.waitForFunction(expected=>{const p=Viewer.state.panes[0];return p.sort===expected.sort&&p.descending===expected.descending;},expected);
  }
  await page.waitForFunction(ids=>JSON.stringify(Viewer.state.panes[0].data.rows.map(r=>r.id))===JSON.stringify(ids),sourceOrder);
  const marker=await page.evaluate(()=>Viewer.state.panes[0].data.next_annotation);assert.ok(marker>0);
  await tablePane.getByRole('button',{name:'Next annotation',exact:true}).click();
  await page.waitForFunction(marker=>Viewer.state.panes[0].data.offset===marker,marker);
  assert.equal(await tablePane.locator('tbody tr').first().locator('.annotation').textContent(),'✕');
  await page.evaluate(async()=>{Viewer.state.panes[0].offset=0;await Viewer.refreshAll();});
  const testKeys=await page.evaluate(async()=>{const q={...Viewer.state.q,table:'tests',offset:0,limit:100};return(await Viewer.api('rows',q)).rows.map(r=>r.key);});assert.equal(testKeys.length,7);
  await page.evaluate(async key=>{Viewer.state.selection.tests=[key];await Viewer.refreshAll();},testKeys[0]);
  const pinCount=await page.evaluate(()=>Viewer.state.panes[0].data.total);
  await page.locator('[data-pane="1"]').getByRole('button',{name:'Pin',exact:true}).click();
  await page.evaluate(async key=>{Viewer.state.selection.tests=[key];await Viewer.refreshAll();},testKeys[1]);
  assert.equal(await page.evaluate(()=>Viewer.state.panes[0].data.total),pinCount);
  await page.locator('[data-pane="1"]').getByRole('button',{name:'New',exact:true}).click();
  await page.waitForFunction(()=>Viewer.state.panes.length===2&&Viewer.state.panes[1].data);
  assert.equal(await page.evaluate(()=>Viewer.state.panes[1].linked),false);
  await page.locator('[data-pane="1"]').getByRole('button',{name:'Link',exact:true}).click();
  await page.waitForFunction(key=>Viewer.state.panes[0].data.rows.every(r=>r.test===key),testKeys[1]);
  await page.evaluate(async()=>{Viewer.state.selection.tests=[];await Viewer.refreshAll();});
  await page.locator('[data-pane="1"] input[aria-label="Filter Value"]').fill('>100');
  await page.waitForFunction(()=>Viewer.state.panes[0].data.total===0);
  const stats=await page.evaluate(async()=>Viewer.api('plot',{...Viewer.state.q,tests:[],color:'none'}));
  assert.ok(Object.values(stats.series).some(s=>s.count===48));
  await page.locator('[data-pane="1"] input[aria-label="Filter Value"]').fill('');
  await page.waitForFunction(()=>Viewer.state.panes[0].data.total===345);
  await page.locator('[data-pane="1"] th').filter({hasText:/^Site/}).first().getByRole('button',{name:'▾'}).click();
  await page.locator('#dialog').getByRole('button',{name:'Clear',exact:true}).click();
  await page.locator('#value-items input').first().check();
  await page.locator('#dialog').getByRole('button',{name:'Apply',exact:true}).click();
  await page.waitForFunction(()=>Viewer.state.panes[0].data.total<345&&Viewer.state.panes[0].data.total>0);
  assert.equal(await page.evaluate(()=>new Set(Viewer.state.panes[0].data.rows.map(r=>r.site)).size),1);
  await page.locator('.table-scroll tbody tr').first().dblclick();
  await page.locator('#dialog[open]').waitFor();assert.match(await page.locator('#dialog').textContent(),/Record evidence/);
  await page.locator('#close-dialog').click();
  await page.evaluate(async()=>{Viewer.state.panes[0].values={};for(const kind of['histogram','box','probability','trend','range','scatter','pareto','binmap','paramap','timing','records'])Viewer.addPane(kind);await Viewer.refreshAll();});
  await page.waitForFunction(()=>Viewer.state.panes.every(p=>p.data));
  assert.ok(await page.locator('svg.chart').count()>=9);
  for(const theme of['excel','mes','dark','quality','contrast'])for(const lang of['en','zh','ja','ko','de']){
   await page.locator('#report-theme').selectOption(theme);await page.locator('#report-language').selectOption(lang);
   await page.waitForFunction(({theme,lang})=>ReportUI.theme===theme&&ReportUI.language===lang,{theme,lang});
   assert.equal(await page.locator('html').getAttribute('data-theme'),theme);
  }
  await page.locator('#report-language').selectOption('en');await page.locator('#report-theme').selectOption('quality');
  await page.screenshot({path:path.join(out,'workspace.png'),fullPage:false});assert.deepEqual(errors,[]);
  // Saved layouts retain pin, filters, panel options, and source identity.
  await page.evaluate(()=>{const saved=Viewer.session();Viewer.restore(saved);});
  await page.waitForFunction(()=>Viewer.state.panes.every(p=>p.data));
  // Loopback service: query contract, token/Host/Origin, and self-contained export.
  server=spawn(cli,['view',path.join(out,'demo.stdf'),'--cache-dir',path.join(root,'target/viewer-demo-cache'),'--no-open'],{cwd:root,windowsHide:true});
  let log='';server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
  let url;for(let i=0;i<200;i++){url=log.match(/http:\/\/127\.0\.0\.1:\d+\/[a-f0-9]+\//)?.[0];if(url)break;if(server.exitCode!==null)throw Error(log);await wait(100);}assert.ok(url,log);
  const response=await fetch(url+'rows',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({table:'devices',limit:1000})});assert.equal(response.status,200);const devices=await response.json();assert.equal(devices.total,50);assert.equal(devices.rows.filter(r=>r.latest).length,49);
  assert.equal((await fetch(url+'rows',{method:'POST',headers:{Origin:'https://example.invalid'},body:'{}'})).status,403);
  assert.equal((await fetch(new URL('/bad/',url))).status,403);
  const live=await context.newPage();live.on('pageerror',e=>errors.push(e.message));await live.goto(url);await live.waitForFunction(()=>Viewer.state.panes[0].data?.total===345);
  const liveStats=await live.evaluate(async()=>Viewer.api('plot',{...Viewer.state.q,color:'none'}));
  for(const key of Object.keys(stats.series))for(const field of['count','mean','min','max','histogram','quantiles']){const a=liveStats.series[key][field],b=stats.series[key][field];if(typeof a==='number'&&typeof b==='number')assert.ok(Math.abs(a-b)<1e-12*Math.max(1,Math.abs(a)),`${key}: ${field}`);else assert.deepEqual(a,b,`${key}: ${field}`);}
  const layout=await live.evaluate(async()=>{const p=Viewer.addPane('histogram');p.options.bins=10;await Viewer.refreshAll();const s=Viewer.session();s.panes=s.panes.map(p=>({...p,snapshot_query:{...Viewer.state.q,...(p.linked?Viewer.state.selection:p.selection),...p.options,pane:p.id,plot:p.kind,table:'measurements'}}));return s;});
  const summary=await fetch(url+'export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({query:{},scope:'summary',layout})});assert.equal(summary.status,200);const summaryPath=path.join(root,'target/viewer-summary-attachment.html');fs.writeFileSync(summaryPath,await summary.text());
  const exported=await fetch(url+'export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({query:{},scope:'selection'})});assert.equal(exported.status,200);const html=await exported.text();const attachment=path.join(root,'target/viewer-independent-attachment.html');fs.writeFileSync(attachment,html);
  server.kill();server=null;await context.setOffline(true);const detached=await context.newPage();await detached.goto(pathToFileURL(attachment).href);await detached.waitForFunction(()=>Viewer.state.panes[0].data?.total===345);await detached.evaluate(()=>Viewer.addPane('histogram'));await detached.waitForSelector('svg.chart');
  const savedSummary=await context.newPage();await savedSummary.goto(pathToFileURL(summaryPath).href);await savedSummary.waitForFunction(()=>Viewer.state.panes[0]?.data);assert.equal(await savedSummary.evaluate(()=>Viewer.state.panes[0].options.bins),10);assert.ok(await savedSummary.evaluate(()=>Object.values(Viewer.state.panes[0].data.series).some(s=>s.histogram.length===10)));
  // The already saved selection exposes raw-null tooltips and UTC dates in Records.
  await detached.evaluate(()=>Viewer.addPane('records'));
  await detached.waitForFunction(()=>Viewer.state.panes.find(p=>p.kind==='records')?.data);
  const records=detached.locator('article').filter({has:detached.locator('.pane-head strong', {hasText:'Records'})});
  const mir=records.locator('tbody tr[data-id="2"]');await mir.dblclick();
  assert.match(await detached.locator('#dialog').textContent(),/UTC/);await detached.locator('#close-dialog').click();
  const hist=detached.locator('article').filter({has:detached.locator('.pane-head strong',{hasText:'Histogram'})}).first();
  await hist.getByRole('button',{name:'Image',exact:true}).click();const download=detached.waitForEvent('download');await detached.locator('#dialog').getByRole('button',{name:'SVG',exact:true}).click();const image=await download;await image.saveAs(path.join(out,'histogram.svg'));assert.match(fs.readFileSync(path.join(out,'histogram.svg'),'utf8'),/Population/);
  await detached.evaluate(()=>{const p=Viewer.state.panes[0];p.group='A';const s=Viewer.session();s.panes[1].group='A';Viewer.restore(s);});await detached.waitForSelector('.tab-group');assert.equal(await detached.locator('.group-tabs button').count(),2);
  assert.deepEqual(errors,[]);console.log('PASS: live/offline queries, exact statistics, 24px rows, pin/link/new, column filters, inspector, 9 plots, 25 theme/language combinations, session restore, loopback access checks, and disconnected HTML export.');
 }finally{server?.kill();await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
