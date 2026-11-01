# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
"""Demonstrate historical EAV catalog verification with project-owned STDF."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release' /
                        ('zstdf-cli.exe' if os.name == 'nt' else 'zstdf-cli'))
    parser.add_argument('--output-dir', type=Path)
    args = parser.parse_args()
    cli = args.cli.resolve()
    if args.output_dir:
        out = args.output_dir.resolve()
        out.mkdir(parents=True, exist_ok=False)
    else:
        (ROOT / 'target').mkdir(exist_ok=True)
        out = Path(tempfile.mkdtemp(prefix='snapshot-demo-', dir=ROOT / 'target'))
    dataset, source_dir = out / 'dataset', out / 'source'
    source = source_dir / 'demo.stdf'

    def command(*argv):
        return subprocess.run([str(cli), *map(str, argv)], check=True,
                              capture_output=True, text=True, encoding='utf-8').stdout

    def convert(units):
        subprocess.run([sys.executable, str(ROOT / 'examples/viewer/generate_demo.py'),
                        '--units', str(units), '--source-only', '--output-dir', str(source_dir)],
                       check=True, capture_output=True, text=True, encoding='utf-8')
        command('convert', source, '--output-dir', dataset, '--layout', 'catalog')
        return json.loads(command('snapshot', 'create', dataset))

    first = convert(2)
    selected = json.loads(command('snapshot', 'select', dataset, first['snapshot_id']))
    if selected != first:
        raise RuntimeError('Selected snapshot does not match the saved capture')
    second = convert(4)
    if json.loads(command('snapshot', 'current', dataset)) != first:
        raise RuntimeError('Conversion or capture unexpectedly changed the selected snapshot')
    command('snapshot', 'select', dataset, second['snapshot_id'])
    if json.loads(command('snapshot', 'current', dataset)) != second:
        raise RuntimeError('Explicit snapshot selection did not take effect')
    if first['snapshot_id'] == second['snapshot_id']:
        raise RuntimeError('Changed source must produce a different captured catalog')
    total = lambda s: sum(item['rows'] for item in s['catalog']['sources'].values())
    if total(first) == total(second):
        raise RuntimeError('Synthetic source sizes must have different EAV row counts')
    current = dataset / '_catalog.json'
    saved = dataset / '_catalog.saved.json'
    missing_raw = source.with_suffix('.saved-stdf')
    current.rename(saved)
    try:
        source.rename(missing_raw)
        try:
            verified = json.loads(command('snapshot', 'verify', dataset, first['snapshot_id']))
            if verified != first:
                raise RuntimeError('Historical selection changed')
            if json.loads(command('snapshot', 'current', dataset)) != second:
                raise RuntimeError('Current snapshot unexpectedly depends on the live catalog')
            command('snapshot', 'select', dataset, first['snapshot_id'])
            if json.loads(command('snapshot', 'current', dataset)) != first:
                raise RuntimeError('Historical snapshot reselection failed')
            command('snapshot', 'select', dataset, second['snapshot_id'])
        finally:
            missing_raw.rename(source)
    finally:
        saved.rename(current)
    result = {'first_snapshot': first['snapshot_id'], 'first_rows': total(first),
              'second_snapshot': second['snapshot_id'], 'second_rows': total(second),
              'verified_without_current_catalog_or_raw': True,
              'selection_is_explicit': True,
              'historical_reselection_without_current_catalog_or_raw': True,
              'current_snapshot': second['snapshot_id'], 'dataset': str(dataset)}
    (out / 'result.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
