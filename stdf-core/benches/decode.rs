// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use stdf_core::decode_all;

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");
    let synthetic = synthetic_stdf(200, 4);
    let synthetic_records = decode_all(&synthetic).0.len() as u64;
    group.throughput(Throughput::Elements(synthetic_records));
    group.bench_function("synthetic_200x4", |b| {
        b.iter(|| decode_all(black_box(&synthetic)))
    });

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/public-stdf/downloads");
    for (name, relative) in [
        ("lot2", "pystdf/lot2.stdf"),
        ("lot3", "pystdf/lot3.stdf"),
        ("a595", "stdfreader/a595.stdf"),
    ] {
        let path = root.join(relative);
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        let records = decode_all(&data).0.len() as u64;
        group.throughput(Throughput::Elements(records));
        group.bench_with_input(BenchmarkId::new("public", name), &data, |b, data| {
            b.iter(|| decode_all(black_box(data)));
        });
    }
    group.finish();
}

fn synthetic_stdf(parts: usize, tests_per_part: usize) -> Vec<u8> {
    let mut data = Vec::new();
    push(&mut data, 0, 10, &[2, 4]);
    for part in 0..parts {
        push(&mut data, 5, 10, &[1, (part % 8) as u8]);
        for test in 0..tests_per_part {
            let mut body = Vec::new();
            body.extend_from_slice(&((part * tests_per_part + test) as u32).to_le_bytes());
            body.extend_from_slice(&[1, (part % 8) as u8, 0, 0]);
            body.extend_from_slice(&((test as f32) + 0.25).to_le_bytes());
            push(&mut data, 15, 10, &body);
        }
    }
    data
}

fn push(buf: &mut Vec<u8>, typ: u8, sub: u8, body: &[u8]) {
    let len = u16::try_from(body.len()).expect("synthetic record fits");
    buf.extend_from_slice(&len.to_le_bytes());
    buf.push(typ);
    buf.push(sub);
    buf.extend_from_slice(body);
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
