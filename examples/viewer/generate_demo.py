"""Generate a single-file viewer demo, or a larger reproducible indexing fixture."""
# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
import argparse
import gzip
import os
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

def record(typ, sub, body):
    return struct.pack('<HBB', len(body), typ, sub) + body

def cn(text):
    b = text.encode('ascii')
    return bytes([len(b)]) + b

def run(start, wafer):
    body = struct.pack('<IIBcccHc', start - 10, start, 1, b'P', b' ', b' ', 0, b' ')
    for text in ['VIEWER_LOT', 'SYNTHETIC_DEVICE', 'LOCAL', 'SIMULATED', 'DEMO_JOB', 'v1']:
        body += cn(text)
    return record(1, 10, body) + record(2, 10, struct.pack('<BBI', 1, 1, start) + cn(wafer))

def ptr(site, num, name, value, units='V', flag=0):
    body = struct.pack('<IBBBBf', num, 1, site, flag, 0, value) + cn(name) + cn('')
    body += bytes([0, 0, 0, 0]) + struct.pack('<ff', 0, 10) + cn(units)
    return record(15, 10, body)

def tests(site, x, ordinal):
    value = 1 + (x % 9) / 2
    data = ptr(site, 100, 'Voltage', value)
    if ordinal % 5 == 0:
        data += ptr(site, 100, 'Voltage', value + .25)  # Retain repeated execution.
    data += ptr(site, 101, 'Leakage', value / 10, 'mA')
    data += ptr(site, 102, 'Reference', value * 1.2)
    flag = 128 if ordinal % 7 == 0 else 0
    body = struct.pack('<IBBBB', 200, 1, site, flag, 0xc0) + bytes(32) + cn('pattern/scan')
    data += record(15, 20, body)
    body = struct.pack('<IBBBBHHff', 300, 1, site, 0, 0, 0, 2, value, value + .5) + cn('Channels')
    data += record(15, 15, body)
    return data

def prr(site, x, ordinal):
    fail = ordinal % 7 == 0
    return record(5, 20, struct.pack('<BBBHHHhhI', 1, site, 8 if fail else 0, 7,
                  9 if fail else 1, 90 if fail else 10, x, 1 + ordinal // 200, 30 + ordinal % 30) + cn('REPEATED_PART_ID'))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release' / ('zstdf-cli.exe' if os.name == 'nt' else 'zstdf-cli'))
    parser.add_argument('--units', type=int, default=48)
    parser.add_argument('--source-only', action='store_true')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'examples/viewer/generated')
    args = parser.parse_args()
    if args.units < 2 or args.units > 1000000:
        parser.error('--units must be between 2 and 1000000')
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source = out / 'demo.stdf'
    with source.open('wb') as f:
        f.write(record(0, 10, bytes([2, 4])))
        f.write(run(1720000100, 'W1'))
        # Two sites interleaved; site 2 completes first.
        for first in range(1, args.units + 1, 2):
            ids = list(range(first, min(first + 2, args.units + 1)))
            for i in ids:
                f.write(record(5, 10, bytes([1, 1 + i % 2])))
            for i in ids:
                f.write(tests(1 + i % 2, 1 + i % 199, i))
            for i in reversed(ids):
                f.write(prr(1 + i % 2, 1 + i % 199, i))
        f.write(record(1, 20, struct.pack('<I', 1720002000)))
        f.write(run(1720000200, 'W1'))
        # Same device in a later START_T run, despite overlapping intervals.
        f.write(record(5, 10, bytes([1, 1])))
        f.write(tests(1, 2, 1))
        f.write(prr(1, 2, 1))
        f.write(record(1, 20, struct.pack('<I', 1720002000)))
        f.write(run(1720000300, 'W2'))
        # PRR-only unit, plus a raw unassigned DTR record.
        f.write(record(50, 30, cn('Synthetic source: no tester measurements implied.')))
        f.write(record(5, 10, bytes([1, 1])))
        f.write(prr(1, 2, 1))
        f.write(record(1, 20, struct.pack('<I', 1720002000)))
    (out / 'demo.stdf.gz').write_bytes(gzip.compress(source.read_bytes(), mtime=0))
    if not args.source_only:
        cli = str(args.cli.resolve())
        for scope in ['selection', 'summary']:
            subprocess.run([cli, 'view', str(source), '--cache-dir', str(ROOT / 'target/viewer-demo-cache'),
                            '--flow', 'CP', '--export-html', str(out / (scope + '.html')),
                            '--export-scope', scope], check=True)
        print((out / 'selection.html').as_uri())
    print(f'{args.units + 2} attempts; 3 runs; 2 wafers; repeated PART_ID; interleaved sites; one retest; one PRR-only unit.')

if __name__ == '__main__':
    main()
