"""Generate synthetic FTR patterns, retests, incomplete results, and duplicate gzip evidence."""
import gzip
import struct
from pathlib import Path


def rec(typ, sub, body):
    return struct.pack('<HBB', len(body), typ, sub) + body


def cn(text):
    value = text.encode('ascii')
    return bytes([len(value)]) + value


def ftr(pattern, flag, site, test=100):
    body = struct.pack('<IBBB', test, 1, site, flag)
    if pattern is not None:
        body += bytes([0xc0]) + bytes(32) + cn(pattern)
    return rec(15, 20, body)


def source(revision, units):
    data = rec(0, 10, bytes([2, 4]))
    mir = struct.pack('<IIBcccHc', 1720000000, 1720000010, 1, b'P', b' ', b' ', 0, b' ')
    for value in ['DEMO_LOT', 'SYNTHETIC', 'SIM_NODE', 'SIMULATED', 'DEMO_FT', revision]:
        mir += cn(value)
    data += rec(1, 10, mir)
    for unit in range(1, units + 1):
        site = 1 + unit % 2
        data += rec(5, 10, bytes([1, site]))
        data += ftr('scan/core_at_speed', 128 if unit % 3 == 0 else 0, site)
        data += ftr('memory/mbist', 0, site, 101)
        data += ftr('io/loopback', 128 if unit % 4 == 0 else 0, site, 102)
        count = 3
        if unit == units:
            data += ftr('io/loopback', 64, site, 102)
            data += ftr('optional/not_executed', 16, site, 103)
            data += ftr('memory/mbist', 1, site, 101)
            data += ftr(None, 128, site, 104)
            for flag in [0, 128, 0]:
                data += ftr('scan/core_at_speed', flag, site)
            count += 7
        data += rec(5, 20, struct.pack('<BBBHHHhhI', 1, site, 0, count, 1, 1, unit, 1, 10) + cn('UNIT' + str(unit)))
    data += rec(1, 20, struct.pack('<I', 1720000100))
    return data


if __name__ == '__main__':
    out = Path(__file__).parent / 'generated'
    out.mkdir(exist_ok=True)
    data = source('r1', 12)
    (out / 'patterns.stdf').write_bytes(data)
    (out / 'patterns.stdf.gz').write_bytes(gzip.compress(data, mtime=0))
    print(out.resolve())
