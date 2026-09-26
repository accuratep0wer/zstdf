#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Black-box checks for the bounded experiment CLI, using synthetic checkouts."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

RUNNER = Path(__file__).with_name('improvement_runner.py').resolve()
PROBE = '''import json, pathlib, sys, time
root = pathlib.Path('.')
d = json.loads((root / 'src/metric.json').read_text())
mode = sys.argv[1]
if mode == 'gate':
    sys.exit(1 if d.get('gate_fail') else 0)
if d.get('sleep'):
    time.sleep(d['sleep'])
if d.get('mutate'):
    (root / 'src/mutation.txt').write_text('unexpected mutation')
if d.get('flood'):
    print('X' * d['flood'])
elif d.get('invalid_json'):
    print('not a metric')
else:
    print(json.dumps({'score': d['value']}))
'''


class ImprovementRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='zstdf-improvement-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.baseline = self.root / 'baseline'
        self.candidate = self.root / 'candidate'
        for checkout in (self.baseline, self.candidate):
            (checkout / 'src').mkdir(parents=True)
            (checkout / 'tools').mkdir()
            (checkout / 'tools/probe.py').write_text(PROBE, encoding='utf-8')
            (checkout / 'src/metric.json').write_text('{"value":100}', encoding='utf-8')
        self.state = self.root / 'state'
        self.contract_path = self.root / 'contract.json'
        self.contract = {
            'schema_version': 1,
            'objective': 'Synthetic metric improvement; not a timing benchmark',
            'source_paths': ['src', 'tools'],
            'allowed_changes': ['src'],
            'protected_paths': ['tools'],
            'gates': [{'name': 'correctness', 'argv': ['{python}', 'tools/probe.py', 'gate']}],
            'benchmark': {'argv': ['{python}', 'tools/probe.py', 'benchmark'],
                          'metric': 'score', 'direction': 'lower',
                          'min_relative_gain': 0.1, 'repeats': 3},
            'limits': {'max_rounds': 3, 'max_no_gain': 2, 'wall_seconds': 60,
                       'command_seconds': 5, 'output_bytes': 65536},
        }

    def invoke(self, *args):
        completed = subprocess.run([sys.executable, str(RUNNER), *map(str, args)],
                                   capture_output=True, text=True, encoding='utf-8', timeout=40)
        try:
            output = json.loads(completed.stdout)
        except json.JSONDecodeError as exc:
            self.fail(f'CLI did not return JSON: {completed.stdout!r}; {completed.stderr!r}: {exc}')
        self.assertIn('status', output)
        return completed.returncode, output

    def initialize(self, expected=0):
        self.contract_path.write_text(json.dumps(self.contract), encoding='utf-8')
        code, payload = self.invoke('init', '--contract', self.contract_path,
                                    '--baseline', self.baseline, '--state', self.state)
        self.assertEqual(code, expected, payload)
        return payload

    def evaluate(self, expected=None):
        code, payload = self.invoke('evaluate', '--state', self.state,
                                    '--candidate', self.candidate)
        if expected is not None:
            self.assertEqual(code, expected, payload)
        return code, payload

    def candidate_data(self, value=80, **extra):
        (self.candidate / 'src/metric.json').write_text(
            json.dumps({'value': value, **extra}), encoding='utf-8')

    def snapshot(self, root):
        return {str(p.relative_to(root)): p.read_bytes() for p in root.rglob('*') if p.is_file()}

    def test_acceptance_preserves_both_checkouts(self):
        self.candidate_data()
        before = [self.snapshot(p) for p in (self.baseline, self.candidate)]
        self.initialize()
        _, outcome = self.evaluate(0)
        self.assertEqual(outcome['status'], 'accepted')
        self.assertEqual(before, [self.snapshot(p) for p in (self.baseline, self.candidate)])
        code, state = self.invoke('status', '--state', self.state)
        self.assertEqual(code, 0, state)

    def test_init_does_not_run_commands(self):
        self.contract['gates'][0]['argv'] = ['this-program-does-not-exist']
        self.initialize()

    def test_init_refuses_existing_directory(self):
        self.state.mkdir()
        sentinel = self.state / 'sentinel'
        sentinel.write_text('keep me')
        self.initialize(1)
        self.assertEqual(sentinel.read_text(), 'keep me')

    def test_unchanged_candidate_does_not_pass(self):
        self.initialize()
        self.evaluate(1)

    def test_gain_below_threshold_rejected(self):
        self.initialize()
        self.candidate_data(95)
        _, outcome = self.evaluate(1)
        self.assertNotEqual(outcome['status'], 'accepted')

    def test_higher_is_better(self):
        self.contract['benchmark']['direction'] = 'higher'
        self.initialize()
        self.candidate_data(120)
        self.evaluate(0)

    def test_incumbent_cannot_regress(self):
        self.initialize()
        self.candidate_data(70)
        self.evaluate(0)
        self.candidate_data(80)
        self.evaluate(1)

    def test_failing_candidate_gate_rejected(self):
        self.initialize()
        self.candidate_data(gate_fail=True)
        self.evaluate(1)

    def test_failing_baseline_gate_rejected(self):
        (self.baseline / 'src/metric.json').write_text('{"value":100,"gate_fail":true}')
        self.initialize()
        self.candidate_data()
        self.evaluate(1)

    def test_protected_file_change_refused(self):
        self.initialize()
        self.candidate_data()
        with (self.candidate / 'tools/probe.py').open('a') as stream:
            stream.write('\n# changed assessment\n')
        self.evaluate(1)

    def test_protected_addition_refused(self):
        self.initialize()
        self.candidate_data()
        (self.candidate / 'tools/new.txt').write_text('addition')
        self.evaluate(1)

    def test_protected_deletion_refused(self):
        self.initialize()
        self.candidate_data()
        (self.candidate / 'tools/probe.py').unlink()
        self.evaluate(1)

    def test_scoped_but_unauthorized_change_refused(self):
        for checkout in (self.baseline, self.candidate):
            (checkout / 'other').mkdir()
            (checkout / 'other/a.txt').write_text('original')
        self.contract['source_paths'].append('other')
        self.initialize()
        self.candidate_data()
        (self.candidate / 'other/a.txt').write_text('changed')
        self.evaluate(1)

    def test_baseline_drift_refused(self):
        self.initialize()
        self.candidate_data()
        (self.baseline / 'src/metric.json').write_text('{"value":200}')
        self.evaluate(1)

    def test_benchmark_source_mutation_stops_campaign(self):
        self.initialize()
        self.candidate_data(mutate=True)
        self.evaluate(1)
        self.evaluate(1)

    def test_invalid_metrics_never_accepted(self):
        for value in (float('nan'), float('inf'), -1, 0, '80', True, None):
            with self.subTest(value=value):
                self.state = self.root / ('state-' + str(value))
                self.initialize()
                self.candidate_data(value)
                self.evaluate(1)

    def test_non_json_output_rejected(self):
        self.initialize()
        self.candidate_data(invalid_json=True)
        self.evaluate(1)

    def test_output_budget(self):
        self.contract['limits']['output_bytes'] = 1024
        self.initialize()
        self.candidate_data(flood=100000)
        self.evaluate(1)

    def test_command_timeout(self):
        self.contract['limits']['command_seconds'] = 0.2
        self.initialize()
        self.candidate_data(sleep=2)
        self.evaluate(1)

    def test_max_rounds(self):
        self.contract['limits']['max_rounds'] = 1
        self.initialize()
        self.candidate_data()
        self.evaluate(0)
        self.candidate_data(60)
        self.evaluate(1)

    def test_consecutive_no_gain_stops(self):
        self.contract['limits']['max_no_gain'] = 1
        self.initialize()
        self.candidate_data(99)
        self.evaluate(1)
        self.candidate_data(60)
        self.evaluate(1)

    def test_state_inside_scope_refused(self):
        self.state = self.baseline / 'src/state'
        self.initialize(1)
        self.assertFalse(self.state.exists())

    def test_parent_escape_refused(self):
        self.contract['source_paths'].append('../escape')
        self.initialize(1)

    def test_absolute_scope_refused(self):
        self.contract['source_paths'].append(str(self.baseline / 'src'))
        self.initialize(1)

    def test_protected_outside_scope_refused(self):
        self.contract['protected_paths'].append('unscoped')
        self.initialize(1)

    def test_missing_scope_refused(self):
        self.contract['source_paths'].append('missing')
        self.initialize(1)

    def test_insufficient_repetitions_refused(self):
        self.contract['benchmark']['repeats'] = 1
        self.initialize(1)

    def test_unknown_contract_version_refused(self):
        self.contract['schema_version'] = 99
        self.initialize(1)

    def test_missing_contract_version_refused(self):
        del self.contract['schema_version']
        self.initialize(1)

    def test_symlink_scope_refused(self):
        try:
            (self.baseline / 'src/link').symlink_to(self.contract_path)
        except OSError as exc:
            self.skipTest(f'Host cannot create symlink: {exc}')
        self.initialize(1)

    def test_frozen_state_corruption_refused(self):
        self.initialize()
        frozen = self.state / 'frozen.json'
        frozen.write_text('{"corrupt":true}')
        self.candidate_data()
        self.evaluate(1)

    def test_final_hash_budget_cannot_promote_candidate(self):
        self.initialize()
        self.candidate_data(70)
        spec = importlib.util.spec_from_file_location('tested_improvement_runner', RUNNER)
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        original_snapshot = runner.snapshot
        clock = [0.0]
        count = [0]

        def snapshot(*args, **kwargs):
            count[0] += 1
            if count[0] == 3:  # Final baseline hash, after successful measurements.
                clock[0] = 61.0
            return original_snapshot(*args, **kwargs)

        def command(argv, cwd, seconds, cap):
            return {'argv': argv, 'cwd': str(cwd), 'returncode': 0,
                    'stdout': json.dumps({'score': 100 if cwd == self.baseline else 70}),
                    'stderr': '', 'error': None}

        with patch.object(runner.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(runner, 'snapshot', side_effect=snapshot), \
             patch.object(runner, 'command', side_effect=command):
            outcome, code = runner.evaluate(SimpleNamespace(state=self.state, candidate=self.candidate))
        self.assertEqual(code, 1)
        self.assertEqual(outcome['status'], 'stopped')
        self.assertIsNone(outcome['incumbent'])

    def test_command_evidence_corruption_refused(self):
        self.initialize()
        self.candidate_data(70)
        self.evaluate(0)
        next(self.state.glob('round-*-command-*.json')).write_text('corrupted')
        code, _ = self.invoke('status', '--state', self.state)
        self.assertEqual(code, 1)

    def test_parallel_lock_and_interruption(self):
        self.contract['limits']['command_seconds'] = 10
        self.initialize()
        self.candidate_data(sleep=1)
        process = subprocess.Popen([sys.executable, str(RUNNER), 'evaluate',
                                    '--state', str(self.state), '--candidate', str(self.candidate)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            time.sleep(0.5)
            self.assertIsNone(process.poll(), 'Evaluation unexpectedly finished before lock probe')
            self.evaluate(1)
        finally:
            process.terminate()
            process.communicate(timeout=15)
        # A terminated coordinator cannot leave a candidate implicitly accepted.
        self.evaluate(1)


if __name__ == '__main__':
    unittest.main()
