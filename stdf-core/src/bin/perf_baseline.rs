use std::env;
use std::time::Instant;

use stdf_core::{decode_with_summary, records};

fn main() {
    let config = Config::from_args(env::args().skip(1));
    let data = build_synthetic_stdf(config.parts, config.tests_per_part);

    let decode_start = Instant::now();
    let decoded = decode_with_summary(&data);
    let decode_elapsed = decode_start.elapsed();

    let iter_start = Instant::now();
    let iter_count = records(&data)
        .expect("synthetic data must have valid FAR")
        .filter_map(Result::ok)
        .count();
    let iter_elapsed = iter_start.elapsed();

    println!("zstdf performance baseline");
    println!("parts={}", config.parts);
    println!("tests_per_part={}", config.tests_per_part);
    println!("bytes={}", data.len());
    println!("decode_records={}", decoded.records.len());
    println!(
        "decode_elapsed_ms={:.3}",
        decode_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "decode_records_per_sec={:.0}",
        decoded.records.len() as f64 / decode_elapsed.as_secs_f64().max(1e-9)
    );
    println!("iter_records={iter_count}");
    println!("iter_elapsed_ms={:.3}", iter_elapsed.as_secs_f64() * 1000.0);
    println!(
        "iter_records_per_sec={:.0}",
        iter_count as f64 / iter_elapsed.as_secs_f64().max(1e-9)
    );
    println!("checksum={}", checksum(&data));
}

#[derive(Debug)]
struct Config {
    parts: usize,
    tests_per_part: usize,
}

impl Config {
    fn from_args(args: impl Iterator<Item = String>) -> Self {
        let mut config = Self {
            parts: 10_000,
            tests_per_part: 5,
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--parts" => config.parts = parse_next(&mut args, "--parts"),
                "--tests-per-part" => {
                    config.tests_per_part = parse_next(&mut args, "--tests-per-part");
                }
                "--help" | "-h" => print_help_and_exit(),
                unknown => fail(&format!("unknown argument: {unknown}")),
            }
        }
        config
    }
}

fn parse_next<T: std::str::FromStr>(
    args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    name: &str,
) -> T {
    let Some(value) = args.next() else {
        fail(&format!("{name} requires a value"));
    };
    value
        .parse()
        .unwrap_or_else(|_| fail(&format!("{name} value is invalid")))
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

fn print_help_and_exit() -> ! {
    println!(
        "Usage: cargo run -p stdf-core --bin perf_baseline -- [--parts N] [--tests-per-part N]"
    );
    std::process::exit(0);
}

fn build_synthetic_stdf(parts: usize, tests_per_part: usize) -> Vec<u8> {
    let mut data = Vec::new();
    push_record(&mut data, 0, 10, &[2, 4]);
    for part in 0..parts {
        push_record(&mut data, 5, 10, &[1, (part % 8) as u8]);
        for test in 0..tests_per_part {
            let mut ptr = Vec::new();
            ptr.extend_from_slice(&((part * tests_per_part + test) as u32).to_le_bytes());
            ptr.push(1);
            ptr.push((part % 8) as u8);
            ptr.push(0);
            ptr.push(0);
            ptr.extend_from_slice(&((test as f32) + 0.25).to_le_bytes());
            push_record(&mut data, 15, 10, &ptr);
        }
        let mut prr = Vec::new();
        prr.push(1);
        prr.push((part % 8) as u8);
        prr.push(0);
        prr.extend_from_slice(&(tests_per_part as u16).to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        push_record(&mut data, 5, 20, &prr);
    }
    data
}

fn push_record(buf: &mut Vec<u8>, typ: u8, sub: u8, body: &[u8]) {
    buf.extend_from_slice(&(body.len() as u16).to_le_bytes());
    buf.push(typ);
    buf.push(sub);
    buf.extend_from_slice(body);
}

fn checksum(data: &[u8]) -> u64 {
    data.iter().fold(0u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(*byte as u64)
    })
}
