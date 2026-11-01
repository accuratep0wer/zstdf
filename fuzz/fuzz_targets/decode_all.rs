// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    stdf_core::decoding_invariants(data).expect("decode entry points diverged");
});
