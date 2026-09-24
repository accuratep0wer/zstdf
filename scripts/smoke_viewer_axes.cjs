// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
// Run after examples/viewer/generate_demo.py; requires Playwright and Edge.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const path=require('node:path');
const fs=require('node:fs');
const {pathToFileURL}=require('node:url');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'examples/viewer/generated');
const cli=path.join(root,'target/release',process.platform==='win32'?'zstdf-cli.exe':'zstdf-cli');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));

async function check(page,url,label){
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(url);await page.waitForFunction(()=>window.Viewer?.state.panes[0]?.data?.total>0);
 const recordCount=await page.evaluate(()=>JSON.parse(document.querySelector('#viewer-boot').textContent).manifest.records);
 assert.match(await page.locator('.table-scroll thead tr').first().locator('th').nth(1).textContent(),/Record type/);
 const kinds=await page.locator('.table-scroll tbody tr td:nth-child(2)').allTextContents();
 for(const kind of ['PTR','MPR','FTR'])assert.ok(kinds.includes(kind),kind);
 // Older saved layouts must acquire the new frozen record-type column.
 await page.evaluate(()=>{const s=Viewer.session();s.panes[0].columns=s.panes[0].columns.filter(c=>c!=='kind');Viewer.restore(s);});
 await page.waitForFunction(()=>Viewer.state.panes[0].columns[0]==='kind');
 assert.match(await page.locator('.table-scroll th.frozen').first().textContent(),/Record type/);
 const tests=await page.evaluate(async()=> (await Viewer.api('rows',{...Viewer.state.q,table:'tests',limit:1000})).rows);
 const voltage=tests.find(t=>t.name.includes('PTR 100 ')).key,leakage=tests.find(t=>t.name.includes('PTR 101 ')).key;
 async function chart(kind,selected=[voltage],options={}){
  await page.evaluate(async({kind,selected,options})=>{
   const s=Viewer.session();s.panes=s.panes.slice(0,1);s.selection={tests:selected,attempts:[],hard_bins:[],soft_bins:[]};Viewer.restore(s);
   const p=Viewer.addPane(kind);p.options={color:'none',...options};await Viewer.refreshAll();
  },{kind,selected,options});
  await page.waitForFunction(()=>Viewer.state.panes[1]?.data);
  const pane=page.locator('article.pane').nth(1),svg=pane.locator('svg.chart').first();await svg.waitFor();
  return {pane,svg,axes:await svg.evaluate(n=>({x:n.querySelector('.axis-label.axis-x').textContent,y:n.querySelector('.axis-label.axis-y').textContent,...n.dataset}))};
 }
 async function integers(svg,axis){
  const ticks=await svg.locator('.axis-tick.axis-'+axis).allTextContents();assert.ok(ticks.length>0);
  for(const t of ticks)assert.match(t,/^-?\d+$/,`${label} integer ${axis}: ${t}`);
 }
 let p=await chart('histogram');assert.match(p.axes.x,/Voltage.*\(V\)/);assert.equal(p.axes.y,'Count');await integers(p.svg,'y');
 // Counts are discrete, while a zoomed secondary axis still reports count / total.
 await page.evaluate(async()=>{Viewer.state.panes[1].axes={ymin:2,ymax:7};await Viewer.refreshAll();});
 const percent=await p.svg.locator('.axis-percent').allTextContents();assert.ok(Math.abs(parseFloat(percent[0])-7/48*100)<.001);
 for(const kind of ['box','range']){
  p=await chart(kind);assert.equal(p.axes.xType,'category');assert.equal(p.axes.x,'Test / series');assert.match(p.axes.y,/\(V\)/);
  assert.match(await p.svg.locator('.axis-tick.axis-x').first().getAttribute('data-category'),/Voltage/);
 }
 p=await chart('probability');assert.match(p.axes.x,/Voltage.*\(V\)/);assert.equal(p.axes.y,'Cumulative probability (%)');
 assert.deepEqual(await p.svg.locator('.axis-tick.axis-y').allTextContents(),['0%','20%','40%','60%','80%','100%']);
 p=await chart('trend');assert.equal(p.axes.x,'Source record sequence');assert.equal(+p.axes.xMin,1);assert.equal(+p.axes.xMax,recordCount);await integers(p.svg,'x');
 assert.match(p.axes.y,/Voltage.*\(V\)/);
 p=await chart('scatter',[voltage,leakage]);assert.match(p.axes.x,/Voltage.*\(V\)/);assert.match(p.axes.y,/Leakage.*\(mA\)/);
 const scatter=await page.evaluate(()=>Viewer.state.panes[1].data);assert.deepEqual(scatter.x_units,['V']);assert.deepEqual(scatter.y_units,['mA']);
 assert.equal(+p.axes.xMin,scatter.x_min);assert.equal(+p.axes.yMax,scatter.y_max);
 for(const kind of ['binmap','paramap']){
  p=await chart(kind);assert.equal(p.axes.x,'X coordinate');assert.equal(p.axes.y,'Y coordinate');await integers(p.svg,'x');await integers(p.svg,'y');
  await p.pane.getByRole('button',{name:'Rotate',exact:true}).click();
  assert.equal(await p.svg.locator('.axis-label.axis-x').textContent(),'− Y coordinate');assert.equal(await p.svg.locator('.axis-label.axis-y').textContent(),'X coordinate');await integers(p.svg,'x');await integers(p.svg,'y');
  await p.pane.getByRole('button',{name:'Flip Y',exact:true}).click();assert.equal(await p.svg.locator('.axis-label.axis-x').textContent(),'Y coordinate');
 }
 p=await chart('binmap',[]);
 assert.match(await p.pane.locator('.map-legend').textContent(),/1 — Description unavailable/);
 const swatches=await p.pane.locator('.map-legend > span > span').evaluateAll(ns=>ns.map(n=>n.style.backgroundColor));
 assert.equal(new Set(swatches).size,swatches.length);
 await p.pane.getByLabel('Map color',{exact:true}).selectOption('soft_bin');
 assert.match(await p.pane.locator('.map-legend').textContent(),/10 — Description unavailable/);
 p=await chart('paramap');
 const gradient=p.pane.locator('.map-gradient');assert.equal(await gradient.count(),1);
 assert.ok(await gradient.evaluate(n=>{const a=n.getBoundingClientRect(),b=n.closest('.pane-body').getBoundingClientRect();return a.top>=b.top&&a.bottom<=b.bottom;}),'Gradient visible without scrolling');
 assert.match(await p.pane.locator('.map-legend').textContent(),/Min:.*Max:/);
 await p.pane.getByLabel('Map endpoint color').fill('#ff0080');
 assert.match(await gradient.getAttribute('style'),/rgb\(255, 0, 128\)|#ff0080/);
 assert.equal(await page.evaluate(()=>Viewer.session().panes[1].mapEndpoint),'#ff0080');
 await page.evaluate(()=>Viewer.restore(Viewer.session()));
 await page.waitForFunction(()=>document.querySelector('input[type=color]')?.value==='#ff0080');
 for(const [level,title]of [['hard_bin','Hard bin'],['soft_bin','Soft bin'],['test','Test'],['pattern','FTR pattern']]){
  p=await chart('pareto',[],{level});assert.equal(p.axes.x,'Device count');assert.equal(p.axes.y,title);assert.equal(p.axes.yType,'category');await integers(p.svg,'x');
 }
 p=await chart('trend');await page.screenshot({path:path.join(out,`axes-${label}.png`)});
 assert.deepEqual(errors,[]);console.log(`PASS ${label}: frozen record types, source sequence endpoints, integer axes, test categories, units, Scatter variables, probability and histogram percentages, rotated coordinates, all Pareto levels.`);
}
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});let server;
 try{
  const context=await browser.newContext({viewport:{width:1550,height:1000}});
  await context.setOffline(true);await check(await context.newPage(),pathToFileURL(path.join(out,'selection.html')).href+'?lang=en','offline');
  // Deliberately inconsistent fixture: one selected Voltage observation uses mV.
  const html=fs.readFileSync(path.join(out,'selection.html'),'utf8'),match=html.match(/(<script[^>]*id="viewer-boot"[^>]*>)([\s\S]*?)(<\/script>)/);
  const boot=JSON.parse(match[2]);boot.rows.find(r=>r.name==='Voltage'&&r.latest&&r.last_execution).units='mV';
  const conflict=path.join(out,'axes-unit-conflict.html');fs.writeFileSync(conflict,html.replace(match[0],match[1]+JSON.stringify(boot).replace(/</g,'\\u003c')+match[3]));
  const mixed=await context.newPage();await mixed.goto(pathToFileURL(conflict).href+'?lang=en');await mixed.waitForFunction(()=>window.Viewer?.state.panes[0]?.data);
  await mixed.evaluate(async()=>{const data=await Viewer.api('rows',{...Viewer.state.q,table:'tests',limit:1000});Viewer.state.selection.tests=[data.rows.find(r=>r.name.includes('PTR 100 ')).key,data.rows.find(r=>r.name.includes('PTR 101 ')).key];Viewer.addPane('scatter');await Viewer.refreshAll();});
  await mixed.waitForFunction(()=>Viewer.state.panes[1]?.data?.unavailable);assert.match(await mixed.locator('article.pane').nth(1).locator('.pane-body').textContent(),/incompatible units/);assert.equal(await mixed.locator('svg.chart').count(),0);await mixed.close();
  await context.setOffline(false);
  server=spawn(cli,['view',path.join(out,'demo.stdf'),'--cache-dir',path.join(root,'target/viewer-demo-cache'),'--no-open'],{cwd:root,windowsHide:true});
  let log='';server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);let url;
  for(let i=0;i<300;i++){url=log.match(/http:\/\/127\.0\.0\.1:\d+\/[a-f0-9]+\//)?.[0];if(url)break;if(server.exitCode!==null)throw Error(log);await sleep(100);}
  assert.ok(url,log);await check(await context.newPage(),url+'?lang=en','live');
 }finally{server?.kill();await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
