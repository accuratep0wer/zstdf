# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
"""Retain native evidence, then inspect it after its original inputs are removed."""
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
        out = Path(tempfile.mkdtemp(prefix='evidence-demo-', dir=ROOT / 'target'))
    source_dir, dataset = out / 'source', out / 'dataset'
    source = source_dir / 'demo.stdf.gz'

    def command(*argv):
        result = subprocess.run([str(cli), *map(str, argv)], capture_output=True,
                                text=True, encoding='utf-8')
        if result.returncode:
            raise RuntimeError(f'{argv[0]} failed: {result.stderr}')
        return result.stdout

    def convert(units):
        subprocess.run([sys.executable, str(ROOT / 'examples/viewer/generate_demo.py'),
                        '--units', str(units), '--source-only', '--output-dir', str(source_dir)],
                       check=True, capture_output=True, text=True, encoding='utf-8')
        command('convert', source, '--output-dir', dataset, '--layout', 'catalog')
        return json.loads(command('snapshot', 'create', dataset, '--select'))

    first = convert(4)
    assert len(first['catalog']['sources']) == 1
    source_id = next(iter(first['catalog']['sources']))
    bundle = json.loads(command('snapshot', 'retain-evidence', dataset,
                                first['snapshot_id'], source_id, source))
    assert bundle['attempts'] == 6 and bundle['measurements'] == 35
    assert bundle['source_sha256'] != bundle['content_sha256']
    second = convert(6)
    assert first['snapshot_id'] != second['snapshot_id']
    # These are only the synthetic files created by this fresh demonstration.
    source.unlink()
    (source_dir / 'demo.stdf').unlink()
    current = dataset / '_catalog.json'
    saved = dataset / '_catalog.saved.json'
    current.rename(saved)
    try:
        verified = json.loads(command('snapshot', 'verify-evidence', dataset, bundle['evidence_id']))
        assert verified == bundle
        attempt = json.loads(command('snapshot', 'evidence-attempt', dataset, bundle['evidence_id'], 6))
        record = json.loads(command('snapshot', 'evidence-record', dataset, bundle['evidence_id'], attempt['record']))
        assert attempt['kind'] == 'PRR' and record['kind'] == 'PRR'
        assert record['offset'] == attempt['offset'] and record['attempt'] == 6
        assert json.loads(command('snapshot', 'current', dataset))['snapshot_id'] == second['snapshot_id']
    finally:
        saved.rename(current)
    result = {
        'evidence_id': bundle['evidence_id'], 'snapshot_id': first['snapshot_id'],
        'attempts': bundle['attempts'], 'executions': bundle['executions'],
        'measurements': bundle['measurements'], 'records': bundle['records'],
        'prr_only_attempt': 6, 'verified_after_original_inputs_removed': True,
        'verified_without_live_catalog': True, 'newer_snapshot_selection_preserved': True,
        'retained_original_gzip_container': False, 'dataset': str(dataset),
    }
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
