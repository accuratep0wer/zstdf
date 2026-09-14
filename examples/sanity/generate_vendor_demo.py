"""Synthetic ATR/CDR/ATER/CTSR/CTRR fixtures; not a tester capture.

Run from the repository root: python -B examples/sanity/generate_vendor_demo.py
"""
import gzip
import struct
from pathlib import Path

from generate_demo import cn, record, sample


def gdr(values):
    body = struct.pack('<H', len(values))
    for kind, value in values:
        if kind == 'Cn':
            body += b'\x0a' + cn(value)
        else:
            tag, fmt = {'U1': (1, 'B'), 'U4': (3, 'I'), 'R8': (8, 'd')}[kind]
            body += bytes([tag]) + struct.pack('<' + fmt, value)
    return record(50, 10, body)


def setup(char_id, margin=False):
    values = [('Cn', s) for s in [
        'MARGIN' if margin else 'SHMOO', str(char_id),
        'Synthetic characterization', 'Demo.Functional', 'Synthetic plot',
        'serial' if margin else 'snakeVertical', 'false',
    ]] + [('U1', 1 if margin else 2)]
    for axis_id in range(1, 2 if margin else 3):
        values += [('U1', axis_id)] + [('Cn', s) for s in [
            'vcc' if axis_id == 1 else 'period', '', 'specVariable',
            'demo.vcc' if axis_id == 1 else 'demo.period',
            '(ALL,1:5.0,2:5.0)', 'absolute', '(4.0,5.0)',
        ]] + [('R8', 0.0), ('U4', 1)]
        values += [('Cn', 'serial'), ('R8', 0.5)] if margin else [('U4', 0), ('Cn', 'linear')]
        values += [('U1', 1)]
        values += [('U1', 1)] + [('Cn', s) for s in [
            'tracking', '', 'specVariable', 'demo.tracked',
            '5.0', 'relative', '(-10%,6%)',
        ]] + [('R8', 0.25)]
    return gdr(values)


def result(char_id, site, x, y, margin=False):
    return gdr([
        ('Cn', 'MARGIN_RESULT' if margin else 'SHMOO_RESULT'),
        ('U4', char_id), ('U1', 1), ('U4', site), ('Cn', '110'),
        ('Cn', f'({x})' if margin else f'({x},{y})'),
        ('Cn', 'Demo.Functional'), ('Cn', 'fail' if x == 0 else 'pass'),
    ])


def build():
    original = sample('cp')
    out = bytearray()
    offset = unit = 0
    while offset < len(original):
        size, typ, sub = struct.unpack_from('<HBB', original, offset)
        raw = original[offset:offset + 4 + size]
        if (typ, sub) == (5, 20):
            site = raw[5]
            margin = unit == 3
            out += setup(unit, margin)
            for x in range(2):
                for y in range(1 if margin else 2):
                    out += result(unit, site, x, y, margin)
        out += raw
        if (typ, sub) == (0, 10):
            out += record(0, 20, struct.pack('<I', 1700000000) + cn('Synthetic fixture generator'))
        if (typ, sub) == (1, 10):
            cell = b'demo.scan.cell'
            cdr = b'\0' + struct.pack('<H', 1) + cn('demo.chain')
            cdr += struct.pack('<IHHBHBHBH', 1, 1, 2, 1, 3, 1, 4, 0, 1)
            cdr += struct.pack('<H', len(cell)) + cell
            out += record(1, 94, cdr)
        if (typ, sub) == (5, 10):
            unit += 1
            out += record(137, 10, cn('ACTIVITY_TRACE_LOG') + cn('Demo.TestMethod')
                          + raw[4:6] + cn(f'Activity for unit {unit}, site {raw[5]}'))
        offset += 4 + size
    return bytes(out)


if __name__ == '__main__':
    target = Path(__file__).parent / 'generated'
    target.mkdir(exist_ok=True)
    data = build()
    (target / 'vendor.stdf').write_bytes(data)
    (target / 'vendor.stdf.gz').write_bytes(gzip.compress(data, mtime=0))
    print(f'Wrote {len(data)} bytes to {target / "vendor.stdf"}')
