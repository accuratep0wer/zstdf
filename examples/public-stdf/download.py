"""Fetch pinned public samples and provenance; verify SHA-256 before publication.

These third-party datasets are not relicensed under zstdf's Apache-2.0 license.
See README.md for the distinction between repository and dataset licensing.
"""
# Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib
import json
import urllib.request

ROOT = Path(__file__).resolve().parent


def main():
    for entry in json.loads((ROOT / 'sources.json').read_text(encoding='utf8')):
        dest = ROOT / 'downloads' / entry['file']
        if dest.exists() and hashlib.sha256(dest.read_bytes()).hexdigest() == entry['sha256']:
            print('Verified:', entry['file'])
            continue
        with urllib.request.urlopen(entry['url'], timeout=60) as response:
            data = response.read(entry['bytes'] + 1)
        if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
            raise ValueError('Download size/hash mismatch: ' + entry['file'])
        dest.parent.mkdir(parents=True, exist_ok=True)
        temp = dest.with_name(dest.name + '.download')
        temp.write_bytes(data)
        temp.replace(dest)
        print('Downloaded:', entry['file'])


if __name__ == '__main__':
    main()
