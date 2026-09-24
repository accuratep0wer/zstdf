// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
// Download first with download.py, then run: node examples/public-stdf/prepare_reports.cjs
const fs=require('node:fs');
const path=require('node:path');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'../..'),out=path.join(__dirname,'generated');
const cli=path.join(root,'target/release',process.platform==='win32'?'zstdf-cli.exe':'zstdf-cli');
const samples=['pystdf/lot2.stdf','pystdf/lot3.stdf','stdfreader/a595.stdf'];
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function prepare(sample){
 const stem=path.basename(sample,'.stdf');
 const child=spawn(cli,['view',path.join(__dirname,'downloads',sample),'--cache-dir',path.join(root,'target/public-stdf-cache'),'--memory-limit-mib','1024','--export-size-mib','100','--no-open'],{cwd:root,windowsHide:true});
 let log='';child.stdout.on('data',d=>log+=d);child.stderr.on('data',d=>log+=d);
 const exited=new Promise(resolve=>child.once('exit',resolve));
 try{
  let url;
  for(let i=0;i<3000;i++){
   url=log.match(/http:\/\/127\.0\.0\.1:\d+\/[a-f0-9]+\//)?.[0];
   if(url)break;if(child.exitCode!==null)throw Error(log);await delay(100);
  }
  if(!url)throw Error('Viewer startup timed out: '+log);
  async function api(command,q){
   const r=await fetch(url+command,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(q),signal:AbortSignal.timeout(300000)});
   const body=await r.text();if(!r.ok)throw Error(body);return command==='export'?body:JSON.parse(body);
  }
  const tests=await api('rows',{table:'tests',population:'all',limit:1000});
  if(tests.total>tests.rows.length)throw Error('Test list needs pagination');
  const ranked=[...tests.rows].sort((a,b)=>b.count-a.count);
  const chosen=ranked.filter(r=>r.name.startsWith('PTR ')&&r.max>r.min).slice(0,3);
  let patternFailures=[];
  for(const test of ranked.filter(r=>r.name.startsWith('FTR ')).sort((a,b)=>b.fail-a.fail)){
   const executions=await api('rows',{table:'measurements',population:'all',tests:[test.key],limit:1000});
   if(executions.rows.some(r=>r.pattern)){
    chosen.push(test);patternFailures=executions.rows.filter(r=>r.passed===false).map(r=>r.attempt);break;
   }
  }
  const devices=await api('rows',{table:'devices',population:'all',limit:1000});
  if(devices.total>10000)throw Error('Demo selector supports at most 10000 attempts');
  while(devices.rows.length<devices.total){
   const next=await api('rows',{table:'devices',population:'all',limit:1000,offset:devices.rows.length});
   if(!next.rows.length)throw Error('Incomplete device pagination');
   devices.rows.push(...next.rows);
  }
  // Explicit small demo scope: up to 12 failed attempts, then earliest other attempts.
  const selected=devices.rows.filter(r=>patternFailures.includes(r.attempt)).slice(0,12);
  for(const r of devices.rows)if(selected.length<12&&r.passed===false&&!selected.some(s=>s.attempt===r.attempt))selected.push(r);
  for(const r of devices.rows)if(selected.length<24&&!selected.some(s=>s.attempt===r.attempt))selected.push(r);
  const q={population:'all',tests:chosen.map(r=>r.key),attempts:selected.map(r=>r.attempt)};
  if(!q.tests.length)throw Error('No suitable tests found');
  const measurements=await api('rows',{...q,limit:2});
  const html=await api('export',{query:q,scope:'selection'});
  const report=path.join(out,stem+'-selection.html');fs.writeFileSync(report,html);
  const result={sample,attempts:devices.total,selected_attempts:q.attempts,tests:tests.total,selected_tests:chosen,selected_measurements:measurements.total,device_issues_preview:devices.rows.slice(0,2).map(r=>r.issue),population:'all',report:path.relative(root,report),report_bytes:Buffer.byteLength(html)};
  console.log(`${stem}: ${result.attempts} attempts, ${result.tests} tests, ${result.selected_measurements} selected measurements; report ${(result.report_bytes/1048576).toFixed(2)} MiB`);
  return result;
 }finally{
  if(child.exitCode===null)child.kill();await exited;
  fs.writeFileSync(path.join(out,stem+'.server.log'),log);
 }
}
(async()=>{
 fs.mkdirSync(out,{recursive:true});const results=[];
 for(const sample of samples)results.push(await prepare(sample));
 fs.writeFileSync(path.join(out,'selected-reports.json'),JSON.stringify(results,null,2)+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
