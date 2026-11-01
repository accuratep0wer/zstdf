use std::env;

use stdf_core::{decode_all, decode_with_summary, detect_byte_order_result, summarize};

const SEEDS: &[&str] = &[
    include_str!("../../../fixtures/stdf/golden/minimal_le.hex"),
    include_str!("../../../fixtures/stdf/golden/minimal_be.hex"),
    include_str!("../../../fixtures/stdf/golden/ptr_prr_flow.hex"),
    include_str!("../../../fixtures/stdf/golden/malformed_truncated.hex"),
    include_str!("../../../fixtures/stdf/golden/unknown_vendor.hex"),
];

fn main() {
    let config = Config::from_args(env::args().skip(1));
    let seeds: Vec<Vec<u8>> = SEEDS.iter().map(|seed| parse_hex_fixture(seed)).collect();
    let mut state = config.seed;
    let mut decoded_records = 0usize;
    let mut reported_errors = 0usize;

    for case_id in 0..config.iterations {
        let seed = &seeds[case_id % seeds.len()];
        let mut data = seed.clone();
        state = mutate_case(&mut data, state, case_id, config.max_len);

        if detect_byte_order_result(&data).is_err() {
            reported_errors += 1;
        }
        let (records, err) = decode_all(&data);
        decoded_records += records.len();
        if err.is_some() {
            reported_errors += 1;
        }
        let decoded = decode_with_summary(&data);
        decoded_records += decoded.records.len();
        if decoded.error.is_some() {
            reported_errors += 1;
        }
        let summary = summarize(&data);
        decoded_records += summary.total_records;
        reported_errors += summary.errors.len();
    }

    println!("fuzz_decode iterations={}", config.iterations);
    println!("fuzz_decode max_len={}", config.max_len);
    println!("fuzz_decode final_seed={state}");
    println!("fuzz_decode decoded_records={decoded_records}");
    println!("fuzz_decode reported_errors={reported_errors}");
}

#[derive(Debug)]
struct Config {
    iterations: usize,
    max_len: usize,
    seed: u32,
}

impl Config {
    fn from_args(args: impl Iterator<Item = String>) -> Self {
        let mut config = Self {
            iterations: 10_000,
            max_len: 4096,
            seed: 0xC0DE_2026,
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--iterations" => {
                    config.iterations = parse_next(&mut args, "--iterations");
                }
                "--max-len" => {
                    config.max_len = parse_next(&mut args, "--max-len");
                }
                "--seed" => {
                    config.seed = parse_next(&mut args, "--seed");
                }
                "--help" | "-h" => {
                    print_help_and_exit();
                }
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
    println!("Usage: cargo run -p stdf-core --bin fuzz_decode -- [--iterations N] [--max-len N] [--seed N]");
    std::process::exit(0);
}

fn mutate_case(data: &mut Vec<u8>, mut state: u32, case_id: usize, max_len: usize) -> u32 {
    state = state
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223)
        .wrapping_add(case_id as u32);
    match case_id % 10 {
        0 if !data.is_empty() => data.truncate((state as usize) % data.len()),
        1 if data.len() < max_len => data.extend_from_slice(&state.to_le_bytes()),
        2 if !data.is_empty() => {
            let index = (state as usize) % data.len();
            data[index] ^= (state >> 24) as u8;
        }
        3 if data.len() >= 2 => {
            data[0] = (state >> 8) as u8;
            data[1] = state as u8;
        }
        4 => data.clear(),
        5 if data.len() > 4 => data[4] = (state % 255) as u8,
        6 if data.len() + 4 <= max_len => {
            data.splice(0..0, [0xFF, 0x00, 0xAA, 0x55]);
        }
        7 => data.reverse(),
        8 if data.len() < max_len => {
            let extra_len = ((state >> 16) as usize % 32).min(max_len - data.len());
            for _ in 0..extra_len {
                state = state.wrapping_mul(22_695_477).wrapping_add(1);
                data.push((state >> 24) as u8);
            }
        }
        _ => {}
    }
    state
}

fn parse_hex_fixture(input: &str) -> Vec<u8> {
    input
        .lines()
        .flat_map(|line| line.split('#').next().unwrap_or("").split_whitespace())
        .map(|token| u8::from_str_radix(token, 16).expect("fixture contains valid hex byte"))
        .collect()
}
