"""Build synthetic inputs and all report demos using an already built zstdf CLI.

Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
Only named generated fixtures/reports are replaced; unrelated files are retained.
"""
import argparse
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default_cli = ROOT / 'target' / 'release' / ('zstdf-cli.exe' if os.name == 'nt' else 'zstdf-cli')
    parser.add_argument('--cli', type=Path, default=default_cli)
    args = parser.parse_args()
    cli = args.cli.resolve()
    if not cli.is_file():
        parser.error('Build the CLI first: cargo build --release -p stdf-cli --locked')

    def run(*command):
        subprocess.run([str(v) for v in command], cwd=ROOT, check=True)

    for generator in ('examples/sanity/generate_demo.py',
                      'examples/sanity/generate_vendor_demo.py',
                      'examples/ftr-patterns/generate_demo.py'):
        run(sys.executable, '-B', ROOT / generator)

    sanity = Path('examples/sanity/generated')
    patterns = Path('examples/ftr-patterns/generated')
    ftr_inputs = [patterns / 'patterns.stdf', patterns / 'patterns.stdf.gz']
    run(cli, 'convert', sanity / 'cp.stdf', patterns / 'cp.parquet')
    run(cli, 'dashboard', patterns / 'cp.parquet', patterns / 'dashboard.html',
        '--ftr-input', ftr_inputs[0], '--ftr-input', ftr_inputs[1])
    run(cli, 'ftr-pareto', *ftr_inputs, '--output', patterns / 'report.html')

    # Use the original mixed PTR/MPR/FTR fixtures; do not strip measurements.
    work = ROOT / 'target' / 'report-demo-inputs'
    work.mkdir(parents=True, exist_ok=True)
    run(cli, 'convert', '--layout', 'catalog', sanity / 'cp.stdf',
        sanity / 'ft.stdf', '--output-dir', work / 'mixed-dataset')
    run(cli, 'dashboard-dir', work / 'mixed-dataset', patterns / 'dataset-dashboard.html',
        '--ftr-input', ftr_inputs[0])

    for domain in ('cp', 'ft'):
        run(cli, 'sanity', sanity / f'{domain}.stdf', '--test-domain', domain,
            '--profile', f'examples/sanity/{domain}-profile.json',
            '--output-dir', sanity / f'{domain}-report')
    run(cli, 'sanity', sanity / 'cp.stdf', sanity / 'ft.stdf',
        '--run-profiles', 'examples/sanity/run-profiles.json',
        '--output-dir', sanity / 'mixed-report')
    run(cli, 'sanity', sanity / 'vendor.stdf', '--test-domain', 'cp',
        '--output-dir', sanity / 'vendor-report')
    run(cli, 'sanity', sanity / 'vendor.stdf', '--test-domain', 'cp',
        '--checks-csv', 'examples/sanity/vendor-checks.csv',
        '--output-dir', sanity / 'vendor-checked-report')
    run(sys.executable, '-B', ROOT / 'examples/traceability/generate_demo.py', '--cli', cli)
    run(sys.executable, '-B', ROOT / 'examples/latest-pareto/generate_demo.py', '--cli', cli)

    print('\nReport demos are ready. Open the Dashboard and select Pareto:')
    print((ROOT / patterns / 'dashboard.html').as_uri() + '?theme=quality&lang=zh&tab=pareto')
    print('Browser checks: node scripts/smoke_report_ui.cjs')


if __name__ == '__main__':
    main()
