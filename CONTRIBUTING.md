# Contributing to zstdf

Unless explicitly stated otherwise, contributions intentionally submitted for
inclusion in zstdf are licensed under the Apache License, Version 2.0, in
accordance with Section 5 of [LICENSE](LICENSE). Contributors retain ownership
of their contributions; submitting a contribution does not assign copyright.

Only submit code, documentation, or test data that you have the right to
contribute under these terms. Preserve existing third-party attribution and
identify any separately licensed material in your pull request. Use synthetic
STDF fixtures for examples; do not submit confidential product or tester data.

For code changes, run the applicable tests and formatting check:

```text
cargo fmt --all --check
cargo test --workspace --exclude stdf-py --locked
```

The root LICENSE and NOTICE are the canonical project notices. Copies in each
active crate ensure that standalone Cargo and Python distributions contain
them. If those notices change, update their package copies and run:

```text
python scripts/check_licenses.py
```
