#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Warn when a diff is larger than the reviewable line budget.

The check is informational. A large diff prints a warning and still exits 0
when the diff can be read. Exit 1 is reserved for a missing git diff.
"""
import argparse
import subprocess
import sys
from pathlib import Path

LIMIT = 800


def is_generated(path):
    norm = path.replace('\\', '/')
    if norm == 'Cargo.lock' or norm.endswith('/Cargo.lock'):
        return True
    parts = norm.split('/')
    return 'generated' in parts


def parse_numstat(text):
    entries = []
    for line in text.splitlines():
        if not line.strip():
            continue
        added, deleted, path = line.split('\t', 2)
        entries.append((path, added, deleted))
    return entries


def line_count(token):
    if token == '-':
        return 1
    return int(token)


def assess(entries):
    """Return a warning string, or None when the diff is inside the budget.

    `entries` are `(path, added, deleted)` numstat fields. Added and deleted
    are decimal strings, or `-` for a binary file.
    """
    ordinary = []
    for path, added, deleted in entries:
        if is_generated(path):
            continue
        ordinary.append((path, line_count(added), line_count(deleted)))
    if not ordinary:
        return None
    if all(added == 0 and deleted > 0 for _, added, deleted in ordinary):
        return None
    churn = sum(added + deleted for _, added, deleted in ordinary)
    if churn <= LIMIT:
        return None
    return (
        f'pull request changes {churn} non-generated lines, above the {LIMIT}-line '
        'review budget. Split the change unless it is a mechanical migration.'
    )


def git_numstat(base, worktree):
    root = Path(__file__).resolve().parents[1]
    if worktree:
        command = ['git', 'diff', '--numstat', 'HEAD']
    else:
        command = ['git', 'diff', '--numstat', base, 'HEAD']
    result = subprocess.run(command, cwd=root, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or 'git diff failed')
    return result.stdout


def emit(message):
    print(f'::warning::{message}')
    print(message, file=sys.stderr)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', default='origin/main', help='tree to diff against HEAD')
    parser.add_argument('--worktree', action='store_true', help='diff the worktree against HEAD')
    args = parser.parse_args(argv)
    try:
        text = git_numstat(args.base, args.worktree)
    except (OSError, RuntimeError) as exc:
        print(f'check_pr_size: {exc}', file=sys.stderr)
        return 1
    message = assess(parse_numstat(text))
    if message:
        emit(message)
    else:
        print(f'check_pr_size: within {LIMIT} non-generated lines or exempt')
    return 0


if __name__ == '__main__':
    sys.exit(main())
