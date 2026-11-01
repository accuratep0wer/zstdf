#!/usr/bin/env python3
# Copyright 2026 zstdf contributors
# SPDX-License-Identifier: Apache-2.0
"""Unit checks for the pull-request size warning."""
import importlib.util
from pathlib import Path
import unittest

MODULE = Path(__file__).with_name('check_pr_size.py')
spec = importlib.util.spec_from_file_location('check_pr_size', MODULE)
check_pr_size = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_pr_size)


class CheckPrSizeTests(unittest.TestCase):
    def test_small_diff_is_quiet(self):
        self.assertIsNone(check_pr_size.assess([
            ('stdf-core/src/lib.rs', '10', '2'),
        ]))

    def test_large_source_diff_warns(self):
        message = check_pr_size.assess([
            ('stdf-core/src/lib.rs', '700', '200'),
        ])
        self.assertIn('900', message)
        self.assertIn('800', message)

    def test_pure_deletion_is_exempt(self):
        self.assertIsNone(check_pr_size.assess([
            ('stdf-cli/src/old.rs', '0', '5000'),
        ]))

    def test_generated_only_is_exempt(self):
        self.assertIsNone(check_pr_size.assess([
            ('Cargo.lock', '4000', '100'),
            ('examples/viewer/generated/demo.html', '9000', '0'),
        ]))

    def test_generated_lines_do_not_hide_a_large_source_diff(self):
        message = check_pr_size.assess([
            ('Cargo.lock', '20', '0'),
            ('stdf-core/src/lib.rs', '801', '0'),
        ])
        self.assertIsNotNone(message)

    def test_binary_token_counts_as_a_line(self):
        self.assertIsNone(check_pr_size.assess([
            ('fixtures/stdf/golden/blob.bin', '-', '-'),
        ]))


if __name__ == '__main__':
    unittest.main()
