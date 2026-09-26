#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Bounded, local experiment evidence runner. Commands are trusted, not sandboxed.

Each candidate compares with the frozen ORIGINAL baseline. Acceptance records
only evidence; it never merges files. Budgets cover active evaluation wall time,
not idle time between invocations. Checksums detect accidental integrity drift;
manual state editing is not a security boundary.
"""
import argparse
import contextlib
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import uuid


class RunnerError(Exception):
    pass


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, allow_nan=False).encode('utf-8')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def atomic_json(path, value):
    data = encoded({'data': value, 'sha256': digest(encoded(value))})
    tmp = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        with tmp.open('xb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def read_json(path):
    try:
        value = json.loads(path.read_text(encoding='utf-8'))
        if value['sha256'] != digest(encoded(value['data'])):
            raise ValueError('checksum mismatch')
        return value['data']
    except (OSError, ValueError, KeyError, TypeError) as exc:
        raise RunnerError(f'invalid evidence {path.name}: {exc}') from exc


def relative(value):
    if not isinstance(value, str) or not value or '\\' in value or ':' in value:
        raise RunnerError(f'invalid relative path: {value!r}')
    parts = value.split('/')
    if any(p in ('', '.', '..') for p in parts) or Path(value).is_absolute():
        raise RunnerError(f'unsafe relative path: {value!r}')
    return value


def covered(name, scope):
    return name == scope or name.startswith(scope + '/')


def clean_path(path):
    """Reject links/junctions in every existing ancestor, before resolving."""
    absolute = Path(os.path.abspath(path))
    for item in (*reversed(absolute.parents), absolute):
        if item.exists() or item.is_symlink():
            st = item.lstat()
            if item.is_symlink() or getattr(st, 'st_file_attributes', 0) & 0x400:
                raise RunnerError(f'symlink/reparse path is forbidden: {item}')
    return absolute


def snapshot(root, scopes, require=False):
    root = clean_path(root)
    if not root.is_dir():
        raise RunnerError(f'checkout is not a directory: {root}')
    files = {}
    def visit(path):
        clean_path(path)
        key = path.relative_to(root).as_posix()
        if path.is_file():
            files[key] = file_hash(path)
        elif path.is_dir():
            # Directory entries detect additions/deletions even when empty.
            files[key + '/'] = 'directory'
            for child in sorted(path.iterdir()):
                visit(child)
        else:
            raise RunnerError(f'unsupported source entry: {path}')
    for scope in scopes:
        path = root / scope
        clean_path(path)
        if path.exists():
            visit(path)
        elif require:
            raise RunnerError(f'missing source scope: {scope}')
    return files


def validate_contract(c):
    if not isinstance(c, dict) or type(c.get('schema_version')) is not int or c['schema_version'] != 1:
        raise RunnerError('contract schema_version must be 1')
    if not isinstance(c.get('objective'), str) or not c['objective'].strip():
        raise RunnerError('objective must be nonempty')
    for key in ('source_paths', 'allowed_changes', 'protected_paths'):
        if not isinstance(c.get(key), list) or (key == 'source_paths' and not c[key]):
            raise RunnerError(f'{key} must be a list (source_paths must be nonempty)')
        for path in c[key]:
            relative(path)
    for key in ('allowed_changes', 'protected_paths'):
        for path in c[key]:
            if not any(covered(path, p) for p in c['source_paths']):
                raise RunnerError(f'{key} path outside source_paths: {path}')
    def argv(command):
        if not isinstance(command, list) or not command or any(not isinstance(x, str) or not x or '\x00' in x for x in command):
            raise RunnerError('argv must be a nonempty list of nonempty strings')
        if any('{' in x.replace('{python}', '') or '}' in x.replace('{python}', '') for x in command):
            raise RunnerError('only {python} placeholder is supported')
    if not isinstance(c.get('gates'), list) or not c['gates']:
        raise RunnerError('at least one correctness gate is required')
    names = set()
    for gate in c['gates']:
        if not isinstance(gate, dict) or not isinstance(gate.get('name'), str) or not gate['name'] or gate['name'] in names:
            raise RunnerError('gate names must be unique nonempty strings')
        names.add(gate['name'])
        argv(gate.get('argv'))
    b = c.get('benchmark', {})
    if not isinstance(b, dict):
        raise RunnerError('benchmark must be an object')
    argv(b.get('argv'))
    if b.get('direction') not in ('lower', 'higher') or not isinstance(b.get('metric'), str) or not b['metric']:
        raise RunnerError('benchmark requires metric and lower/higher direction')
    positive(b.get('min_relative_gain'), 'min_relative_gain')
    if type(b.get('repeats')) is not int or b['repeats'] < 3:
        raise RunnerError('benchmark repeats must be integer >= 3')
    limits = c.get('limits', {})
    if not isinstance(limits, dict):
        raise RunnerError('limits must be an object')
    for key in ('max_rounds', 'max_no_gain', 'output_bytes'):
        if type(limits.get(key)) is not int or limits[key] <= 0:
            raise RunnerError(f'{key} must be a positive integer')
    for key in ('wall_seconds', 'command_seconds'):
        positive(limits.get(key), key)
    return c


def positive(value, name):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        raise RunnerError(f'{name} must be finite and positive')


def environment():
    return {'python': sys.version, 'executable': sys.executable,
            'platform': platform.platform(), 'machine': platform.machine()}


@contextlib.contextmanager
def lock_state(root):
    clean_path(root / 'lock')
    with (root / 'lock').open('a+b') as stream:
        stream.seek(0)
        if os.name == 'nt':
            import msvcrt
            if stream.read(1) == b'':
                stream.write(b'0')
                stream.flush()
            stream.seek(0)
            try:
                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError as exc:
                raise RunnerError('state is locked by another evaluation') from exc
            try:
                yield
            finally:
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError as exc:
                raise RunnerError('state is locked by another evaluation') from exc
            try:
                yield
            finally:
                fcntl.flock(stream, fcntl.LOCK_UN)


def verify(root):
    clean_path(root / 'frozen.json')
    clean_path(root / 'state.json')
    frozen = read_json(root / 'frozen.json')
    state = read_json(root / 'state.json')
    if state['frozen_hash'] != digest(encoded(frozen)):
        raise RunnerError('frozen configuration drift')
    if frozen['runner_hash'] != file_hash(Path(__file__)) or frozen['environment'] != environment():
        raise RunnerError('runner/environment drift; start a new campaign')
    for entry in state['rounds']:
        path = root / relative(entry['path'])
        clean_path(path)
        if file_hash(path) != entry['sha256']:
            raise RunnerError('round evidence drift')
        result = read_json(path)
        for log in result.get('logs', []):
            log_path = root / relative(log['path'])
            clean_path(log_path)
            if file_hash(log_path) != log['sha256']:
                raise RunnerError('command log drift')
    return frozen, state


def init(args):
    contract = validate_contract(json.loads(Path(args.contract).read_text(encoding='utf-8')))
    baseline, root = clean_path(args.baseline), clean_path(args.state)
    for scope in contract['source_paths']:
        if root == baseline / scope or baseline / scope in root.parents:
            raise RunnerError('state must be outside source scope')
    hashes = snapshot(baseline, contract['source_paths'], require=True)
    for p in contract['protected_paths']:
        if not any(covered(k.rstrip('/'), p) for k in hashes):
            raise RunnerError(f'protected path does not exist: {p}')
    if root.exists():
        raise RunnerError('state directory already exists; refusing overwrite')
    root.parent.mkdir(parents=True, exist_ok=True)
    temp = Path(tempfile.mkdtemp(prefix='.improvement-', dir=root.parent))
    try:
        frozen = {'version': 1, 'contract': contract, 'baseline': str(baseline),
                  'hashes': hashes, 'runner_hash': file_hash(Path(__file__)),
                  'environment': environment()}
        atomic_json(temp / 'frozen.json', frozen)
        state = {'status': 'ready', 'frozen_hash': digest(encoded(frozen)), 'rounds': [],
                 'elapsed_seconds': 0.0, 'no_gain': 0, 'incumbent': None, 'reason': ''}
        atomic_json(temp / 'state.json', state)
        # Publish the complete directory in one rename on the same filesystem.
        if root.exists():
            raise RunnerError('state directory already exists; refusing overwrite')
        os.rename(temp, root)
        return state, 0
    except BaseException:
        for path in temp.iterdir() if temp.exists() else []:
            path.unlink()
        if temp.exists():
            temp.rmdir()
        raise


def kill_owned(process):
    if os.name == 'nt':
        subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process.poll() is None:
        process.kill()
    process.wait(timeout=10)


def command(argv, cwd, seconds, cap):
    if seconds <= 0:
        raise RunnerError('campaign wall budget exhausted')
    argv = [x.replace('{python}', sys.executable) for x in argv]
    process = subprocess.Popen(argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=os.name != 'nt')
    buffers = [bytearray(), bytearray()]
    overflow = threading.Event()
    mutex = threading.Lock()
    total = [0]
    def drain(pipe, buf):
        try:
            while True:
                chunk = pipe.read1(4096)
                if not chunk:
                    break
                with mutex:
                    remaining = max(0, cap - total[0])
                    buf.extend(chunk[:remaining])
                    total[0] += len(chunk)
                    if total[0] > cap:
                        overflow.set()
        finally:
            pipe.close()
    threads = [threading.Thread(target=drain, args=(pipe, buf), daemon=True)
               for pipe, buf in zip((process.stdout, process.stderr), buffers)]
    for thread in threads:
        thread.start()
    deadline = time.monotonic() + seconds
    failure = None
    try:
        while process.poll() is None or any(t.is_alive() for t in threads):
            if overflow.is_set():
                failure = 'command output limit exceeded'
                break
            if time.monotonic() >= deadline:
                failure = 'command timeout or campaign wall budget exhausted'
                break
            time.sleep(0.01)
        if failure:
            kill_owned(process)
    except BaseException:
        kill_owned(process)
        raise
    for thread in threads:
        thread.join(timeout=1)
    if any(t.is_alive() for t in threads):
        failure = failure or 'command descendants retained output pipes'
    if overflow.is_set():
        failure = 'command output limit exceeded'
    return {'argv': argv, 'cwd': str(cwd), 'returncode': process.returncode,
            'stdout': bytes(buffers[0]).decode('utf-8', errors='replace'),
            'stderr': bytes(buffers[1]).decode('utf-8', errors='replace'), 'error': failure}


def evaluate(args):
    root = clean_path(args.state)
    with lock_state(root):
        started = time.monotonic()
        frozen, state = verify(root)
        if state['status'] == 'running':
            state.update(status='stopped', reason='interrupted evaluation; incomplete evidence')
            atomic_json(root / 'state.json', state)
            return state, 1
        if state['status'] in ('stopped', 'error'):
            return state, 1
        c = frozen['contract']
        limits = c['limits']
        if len(state['rounds']) >= limits['max_rounds'] or state['no_gain'] >= limits['max_no_gain'] or state['elapsed_seconds'] >= limits['wall_seconds']:
            state.update(status='stopped', reason='campaign budget reached')
            atomic_json(root / 'state.json', state)
            return state, 1
        baseline, candidate = Path(frozen['baseline']), clean_path(args.candidate)
        for scope in c['source_paths']:
            if root == candidate / scope or candidate / scope in root.parents:
                raise RunnerError('state must be outside candidate source scope')
        if snapshot(baseline, c['source_paths']) != frozen['hashes']:
            state.update(status='error', reason='baseline source mutation')
            atomic_json(root / 'state.json', state)
            return state, 1
        hashes = snapshot(candidate, c['source_paths'])
        changed = sorted(k for k in set(hashes) | set(frozen['hashes']) if hashes.get(k) != frozen['hashes'].get(k))
        for key in changed:
            name = key.rstrip('/')
            if any(covered(name, p) for p in c['protected_paths']):
                state.update(status='error', reason=f'protected path changed: {name}')
                atomic_json(root / 'state.json', state)
                return state, 1
            if not any(covered(name, p) for p in c['allowed_changes']):
                state.update(status='error', reason=f'forbidden path changed: {name}')
                atomic_json(root / 'state.json', state)
                return state, 1
        state.update(status='running', candidate=str(candidate), reason='')
        atomic_json(root / 'state.json', state)
        number = len(state['rounds']) + 1
        result = {'round': number, 'candidate': str(candidate), 'hashes': hashes,
                  'changed': changed, 'logs': [], 'status': 'rejected', 'reason': '', 'samples': {'baseline': [], 'candidate': []}}
        def run(argv, checkout, label):
            remaining = limits['wall_seconds'] - state['elapsed_seconds'] - (time.monotonic() - started)
            output = command(argv, checkout, min(limits['command_seconds'], remaining), limits['output_bytes'])
            path = root / f'round-{number:03d}-command-{len(result["logs"]):03d}.json'
            atomic_json(path, output)
            result['logs'].append({'path': path.name, 'sha256': file_hash(path), 'label': label})
            if output['error'] or output['returncode'] != 0:
                raise RunnerError(output['error'] or f'{label} failed with exit {output["returncode"]}')
            return output['stdout']
        try:
            if not changed:
                raise RunnerError('unchanged candidate')
            for gate in c['gates']:
                for label, checkout in [('baseline', baseline), ('candidate', candidate)]:
                    run(gate['argv'], checkout, label + ':' + gate['name'])
            b = c['benchmark']
            for repeat in range(b['repeats']):
                pairs = [('baseline', baseline), ('candidate', candidate)]
                if repeat % 2:
                    pairs.reverse()
                for label, checkout in pairs:
                    stdout = run(b['argv'], checkout, label + ':benchmark')
                    try:
                        value = json.loads(stdout)[b['metric']]
                        positive(value, 'benchmark metric')
                    except (ValueError, KeyError, TypeError) as exc:
                        raise RunnerError('invalid benchmark JSON/metric') from exc
                    result['samples'][label].append(value)
            base = statistics.median(result['samples']['baseline'])
            metric = statistics.median(result['samples']['candidate'])
            positive(base, 'baseline median')
            positive(metric, 'candidate median')
            sign = 1 if b['direction'] == 'lower' else -1
            gain = sign * (base - metric) / base
            if not math.isfinite(gain):
                raise RunnerError('nonfinite relative gain')
            result.update(baseline_median=base, candidate_median=metric, relative_gain=gain)
            incumbent = state['incumbent']
            if gain < b['min_relative_gain']:
                result['reason'] = 'minimum relative gain not reached'
            elif incumbent and sign * (incumbent['metric'] - metric) <= 0:
                result['reason'] = 'candidate does not improve incumbent metric'
            else:
                result.update(status='accepted', reason='gates passed and measured gain accepted')
        except RunnerError as exc:
            result['reason'] = str(exc)
        except (OSError, ValueError) as exc:
            result.update(status='error', reason=str(exc))
        # Source integrity is a hard stop even when a correctness gate failed.
        try:
            if snapshot(baseline, c['source_paths']) != frozen['hashes'] or snapshot(candidate, c['source_paths']) != hashes:
                result.update(status='error', reason='source mutation during evaluation')
        except (RunnerError, OSError) as exc:
            result.update(status='error', reason=f'source integrity failure: {exc}')
        # Charge final integrity hashing before any incumbent promotion.
        state['elapsed_seconds'] += time.monotonic() - started
        if state['elapsed_seconds'] >= limits['wall_seconds'] and result['status'] != 'error':
            result.update(status='stopped', reason='campaign wall budget exhausted')
        state['no_gain'] = 0 if result['status'] == 'accepted' else state['no_gain'] + 1
        if result['status'] == 'accepted':
            state['incumbent'] = {'path': str(candidate), 'hashes': hashes, 'metric': result['candidate_median'], 'round': number}
        state.update(status=result['status'], reason=result['reason'])
        if result['status'] not in ('error', 'stopped') and (number >= limits['max_rounds'] or state['no_gain'] >= limits['max_no_gain']):
            state.update(status='stopped', reason='campaign round/no-gain budget reached')
        path = root / f'round-{number:03d}.json'
        atomic_json(path, result)
        state['rounds'].append({'path': path.name, 'sha256': file_hash(path)})
        atomic_json(root / 'state.json', state)
        return {'status': state['status'], 'reason': state['reason'], 'result': result, 'incumbent': state['incumbent']}, 0 if result['status'] == 'accepted' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='action', required=True)
    p = commands.add_parser('init')
    p.add_argument('--contract', required=True)
    p.add_argument('--baseline', required=True)
    p.add_argument('--state', required=True)
    p = commands.add_parser('evaluate')
    p.add_argument('--state', required=True)
    p.add_argument('--candidate', required=True)
    p = commands.add_parser('status')
    p.add_argument('--state', required=True)
    args = parser.parse_args()
    try:
        if args.action == 'init':
            value, code = init(args)
        elif args.action == 'evaluate':
            value, code = evaluate(args)
        else:
            with lock_state(clean_path(args.state)):
                _, value = verify(clean_path(args.state))
            code = 0
        print(json.dumps(value, ensure_ascii=False, allow_nan=False))
        return code
    except (RunnerError, OSError, ValueError, KeyError, TypeError) as exc:
        print(json.dumps({'status': 'error', 'reason': str(exc)}))
        return 1
    except KeyboardInterrupt:
        print(json.dumps({'status': 'stopped', 'reason': 'interrupted; next evaluate marks campaign incomplete'}))
        return 130


if __name__ == '__main__':
    sys.exit(main())
