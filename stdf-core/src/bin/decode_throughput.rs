// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Measure decode throughput of the pinned public STDF samples.
//!
//! Prints one JSON object on stdout. The improvement runner reads
//! `records_per_sec`. Timing is inside this process, after one warmup decode.

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use stdf_core::decode_all;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/public-stdf/downloads");
    let files = [
        "pystdf/lot2.stdf",
        "pystdf/lot3.stdf",
        "stdfreader/a595.stdf",
    ];
    let mut records = 0u64;
    let mut seconds = 0.0;
    for file in files {
        let path = root.join(file);
        let data = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
        let (count, elapsed) = measure(&data)?;
        records += count;
        seconds += elapsed;
    }
    if seconds <= 0.0 || records == 0 {
        return Err("decode throughput measurement was empty".into());
    }
    let rate = records as f64 / seconds;
    println!("{{\"records_per_sec\":{rate},\"records\":{records},\"seconds\":{seconds}}}");
    Ok(())
}

fn measure(data: &[u8]) -> Result<(u64, f64), String> {
    let (warm, warm_err) = decode_all(black_box(data));
    if warm_err.is_some() {
        return Err("pinned sample did not decode cleanly".into());
    }
    let per_iter = warm.len() as u64;
    let iters = 8u64;
    let start = Instant::now();
    for _ in 0..iters {
        let (decoded, err) = decode_all(black_box(data));
        if err.is_some() || decoded.len() as u64 != per_iter {
            return Err("pinned sample decode changed during measurement".into());
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        return Err("decode interval was zero".into());
    }
    Ok((per_iter * iters, elapsed))
}
