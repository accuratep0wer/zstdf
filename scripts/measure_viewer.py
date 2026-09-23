"""Measure a viewer's indexing time, peak process memory, cache bytes, and queries.
Requires psutil. Uses an owned subprocess and a fresh cache directory; never deletes a cache.
"""
# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
import argparse
import json
from pathlib import Path
import subprocess
import threading
import time
import psutil

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('input', type=Path)
p.add_argument('--cli', type=Path, default=Path('target/release/zstdf-cli.exe'))
p.add_argument('--memory-limit-mib', type=int, default=64)
p.add_argument('--disk-limit-mib', type=int, default=2048)
p.add_argument('--output', type=Path, default=Path('target/viewer-performance.json'))
a = p.parse_args()
root = a.output.parent / ('viewer-bench-' + str(time.time_ns()))
root.mkdir(parents=True)
cancel = root / 'cancel.flag'
result = {'input': str(a.input), 'input_bytes': a.input.stat().st_size,
          'memory_limit_mib': a.memory_limit_mib, 'disk_limit_mib': a.disk_limit_mib}
start = time.perf_counter()
proc = subprocess.Popen([str(a.cli.resolve()), 'view', str(a.input.resolve()), '--cache-dir', str(root / 'cache'),
                        '--memory-limit-mib', str(a.memory_limit_mib), '--disk-limit-mib', str(a.disk_limit_mib),
                        '--no-open', '--cancel-file', str(cancel)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                        text=True, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
peak = [0]
stop = threading.Event()
def monitor():
    while not stop.wait(.02):
        try:
            info = psutil.Process(proc.pid).memory_info()
            peak[0] = max(peak[0], info.rss, getattr(info, 'peak_wset', 0))
        except psutil.Error:
            return
thread = threading.Thread(target=monitor, daemon=True)
thread.start()
try:
    url = None
    log = []
    for line in proc.stdout:
        log.append(line)
        if line.startswith('Open http://'):
            url = line.split()[1]
            break
    result['index_seconds'] = time.perf_counter() - start
    print('Indexed in', result['index_seconds'], 'seconds', flush=True)
    if not url:
        raise RuntimeError(''.join(log))
    result['queries'] = {}
    for name, command, query in [('data_page', 'rows', {'limit':80}),
                                  ('next_data_page', 'rows', {'limit':80,'offset':80}),
                                  ('value_sort', 'rows', {'sort':'value', 'limit':80}),
                                  ('histogram_exact', 'plot', {'plot':'histogram', 'color':'none'}),
                                  ('pareto', 'plot', {'plot':'pareto'})]:
        print('Query', name, flush=True)
        t = time.perf_counter()
        # Use the same Fetch HTTP client as the browser smoke checks.
        client = "fetch(process.argv[1],{method:'POST',headers:{'Content-Type':'application/json'},body:process.argv[2]}).then(async r=>{let b=await r.text();if(!r.ok)throw Error(b);console.log(Buffer.byteLength(b));}).catch(e=>{console.error(e);process.exitCode=1;})"
        response = subprocess.run(['node','-e',client,url+command,json.dumps(query)],capture_output=True,text=True,timeout=600)
        if response.returncode:
            raise RuntimeError(response.stderr)
        result['queries'][name] = {'seconds':time.perf_counter()-t, 'response_bytes':int(response.stdout)}
    result['cache_and_query_bytes'] = sum(f.stat().st_size for f in root.rglob('*') if f.is_file())
    result['peak_rss_bytes'] = peak[0]
    result['note'] = 'Observed process RSS, not a guarantee of a hard OS memory ceiling. Scratch quota and buffer budgets are enforced separately.'
    a.output.write_text(json.dumps(result, indent=2)+'\n', encoding='utf8')
    print(json.dumps(result, indent=2))
finally:
    result['peak_rss_bytes']=peak[0]
    result['cache_and_query_bytes']=sum(f.stat().st_size for f in root.rglob('*') if f.is_file())
    cancel.write_text('stop', encoding='utf8')
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    stop.set()
    thread.join()
    result['persistent_cache_bytes']=sum(f.stat().st_size for f in (root / 'cache').rglob('*') if f.is_file())
    a.output.write_text(json.dumps(result, indent=2)+'\n',encoding='utf8')
