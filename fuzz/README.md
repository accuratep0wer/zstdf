# Decoder fuzz targets

`decode_all` checks that `decode_all`, `decode_with_summary`, and `summarize`
agree. `truncate_prefix` checks that a prefix of an input does not change
records already decoded from the whole input. Both call the oracles in
`stdf-core`'s `invariants` module.

Seeds come from `fixtures/stdf/golden` and, when the pinned public files have
been downloaded, their first 64 KiB. Generate them with:

```text
python3 fuzz/seed_corpus.py
```

A short libFuzzer smoke, from this directory, on nightly:

```text
cargo +nightly fuzz run decode_all -- -max_total_time=20
cargo +nightly fuzz run truncate_prefix -- -max_total_time=20
```

`cargo test -p stdf-core --test properties` runs the same oracles without
libFuzzer. The existing `fuzz_decode` bin remains the fast deterministic smoke.
