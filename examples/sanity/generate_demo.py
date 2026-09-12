"""Generate small synthetic base-v4 CP/FT files; no third-party dependencies."""
import argparse
import gzip
import struct
from pathlib import Path


def record(typ, sub, body):
    return struct.pack('<HBB', len(body), typ, sub) + body


def cn(text):
    encoded = text.encode('ascii')
    return bytes([len(encoded)]) + encoded


def sample(domain, bad=False):
    out = record(0, 10, bytes([2, 4]))
    mir = struct.pack('<IIBcccHc', 100, 110, 1, b'P', b' ', b' ', 0, b' ')
    for text in ['LOT001', 'DEMO', 'NODE1', 'SIMULATED', 'DEMO_' + domain.upper(), 'v1', '', '', '', '', domain.upper(), '25']:
        mir += cn(text)
    out += record(1, 10, mir)
    out += record(1, 80, bytes([1, 1, 2, 1, 2]))
    if domain == 'cp':
        out += record(2, 10, struct.pack('<BBI', 1, 1, 110) + cn('W1'))
    for unit in range(1, 4):
        site = 1 if unit % 2 else 2
        out += record(5, 10, bytes([1, site]))
        out += record(50, 30, cn('Unit fixture ' + str(unit)))
        for index, value in enumerate([1.25, 2.5, 3.75]):
            result = float('nan') if bad and unit == 3 and index == 2 else value
            out += record(15, 10, struct.pack('<IBBBBf', 100 + index, 1, site, 0, 0, result) + cn('VDD_' + str(index)))
            values = struct.pack('<ff', value, value + 0.01)
            out += record(15, 15, struct.pack('<IBBBBHH', 200 + index, 1, site, 0, 0, 0, 2) + values)
        # Tagged double and a function test are visible without expanding the full unit.
        out += record(50, 10, struct.pack('<HBd', 1, 8, 1.0000000000000002))
        out += record(15, 20, struct.pack('<IBBB', 300, 1, site, 0))
        out += record(5, 20, struct.pack('<BBBHHHhhI', 1, site, 0, 7, 1, 1, unit, 1, 10) + cn('UNIT' + str(unit)))
    if domain == 'cp':
        out += record(2, 20, struct.pack('<BBIIIIII', 1, 1, 190, 3, 0, 0, 3, 3) + cn('W1'))
    out += record(1, 30, struct.pack('<BBIIIII', 1, 255, 3, 0, 0, 3, 3))
    out += record(1, 20, struct.pack('<I', 200))
    return out


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output-dir', type=Path, default=Path(__file__).parent / 'generated')
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for domain in ['cp', 'ft']:
        data = sample(domain)
        (args.output_dir / (domain + '.stdf')).write_bytes(data)
        (args.output_dir / (domain + '.stdf.gz')).write_bytes(gzip.compress(data, mtime=0))
    (args.output_dir / 'ft-invalid.stdf').write_bytes(sample('ft', bad=True))
    print(args.output_dir.resolve())
