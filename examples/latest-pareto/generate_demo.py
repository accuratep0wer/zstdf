"""Generate a two-lot latest-attempt Pareto demo with known expected counts."""
import argparse
import gzip
import os
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def rec(typ, sub, body):
    return struct.pack('<HBB', len(body), typ, sub) + body


def cn(value):
    b = value.encode('ascii')
    return bytes([len(b)]) + b


def source(lot, wafer, time, modes):
    data = rec(0, 10, bytes([2, 4]))
    mir = struct.pack('<IIBcccHc', time - 5, time, 1, b'P', b' ', b' ', 0, b' ')
    for s in [lot, 'DEMO_DEVICE', 'SIM_NODE', 'SIM_TESTER', 'DEMO_CP', 'v1']:
        mir += cn(s)
    data += rec(1, 10, mir)
    data += rec(2, 10, struct.pack('<BBI', 1, 1, time) + cn(wafer))
    for x, failed in modes:
        site = 1 + x % 2
        data += rec(5, 10, bytes([1, site]))
        for mode, num, name in [('scan', 100, 'IDD'), ('leak', 101, 'Leakage'), ('timing', 102, 'Timing')]:
            flag = 128 if failed == mode else 0
            data += rec(15, 10, struct.pack('<IBBBBf', num, 1, site, flag, 0, 1.0) + cn(name))
            body = struct.pack('<IBBBB', num + 100, 1, site, flag, 0xc0) + bytes(32) + cn('pattern/' + mode)
            data += rec(15, 20, body)
        bin_num = {None: 1, 'scan': 9, 'leak': 10, 'timing': 11}[failed]
        data += rec(5, 20, struct.pack('<BBBHHHhhI', 1, site, 8 if failed else 0, 6, bin_num, bin_num * 10, x, 1, 10) + cn('SAME_PART_ID'))
    # All run intervals overlap deliberately: START_T alone selects the latest run.
    data += rec(1, 20, struct.pack('<I', 1720010000))
    return data


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli', type=Path, default=ROOT / 'target/release' / ('zstdf-cli.exe' if os.name == 'nt' else 'zstdf-cli'))
    args = p.parse_args()
    cli = args.cli.resolve()
    out = ROOT / 'examples/latest-pareto/generated'
    out.mkdir(parents=True, exist_ok=True)
    cases = [
        ('lot-a-old', 'LOT_A', 'W1', 1720000100, [(1, 'scan'), (2, 'leak'), (3, None), (4, 'timing')]),
        ('lot-a-new', 'LOT_A', 'W1', 1720000200, [(1, None), (2, 'leak'), (3, 'timing'), (4, 'scan'), (4, None)]),
        ('lot-b-old', 'LOT_B', 'W2', 1720000150, [(1, None), (2, None), (3, 'scan')]),
        ('lot-b-new', 'LOT_B', 'W2', 1720000250, [(1, 'scan'), (2, 'leak'), (3, None)]),
    ]
    inputs = []
    for name, lot, wafer, time, modes in cases:
        path = out / (name + '.stdf')
        path.write_bytes(source(lot, wafer, time, modes))
        inputs.append(path)
    (out / 'lot-a-new.stdf.gz').write_bytes(gzip.compress(inputs[1].read_bytes(), mtime=0))
    dataset = ROOT / 'target/latest-pareto-demo-dataset'
    subprocess.run([str(cli), 'convert', *map(str, inputs), '--layout', 'catalog', '--output-dir', str(dataset)], check=True)
    subprocess.run([str(cli), 'dashboard-dir', str(dataset), str(out / 'dashboard.html'), '--title', 'Latest-device Pareto · two-lot retest demo'], check=True)
    print('Expected: 7 latest devices, 8 earlier attempts, 3 passing devices, 4 failing devices.')
    print('Fail bins: HBIN 9=1, HBIN 10=2, HBIN 11=1. Passing bin: HBIN 1=3.')
    print((out / 'dashboard.html').as_uri() + '?tab=pareto&theme=quality&lang=en')


if __name__ == '__main__':
    main()
