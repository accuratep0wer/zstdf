// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Decode oracles for tests and fuzz targets.
//!
//! These functions compare the three public decode entry points. They return
//! an error string instead of panicking so a fuzzer can turn a mismatch into
//! a crash without the decoder itself panicking.

use crate::{decode_all, decode_with_summary, summarize, StdfRecord};

/// `decode_all`, `decode_with_summary`, and `summarize` agree on record count,
/// record types, byte order, truncation, and how many bytes were consumed.
pub fn decoding_invariants(data: &[u8]) -> std::result::Result<(), String> {
    let (records, error) = decode_all(data);
    let decoded = decode_with_summary(data);
    let summary = summarize(data);

    if records.len() != decoded.records.len() {
        return Err(format!(
            "decode_all has {} records and decode_with_summary has {}",
            records.len(),
            decoded.records.len()
        ));
    }
    if summary.total_records != records.len() {
        return Err(format!(
            "summarize counted {} records and decode_all returned {}",
            summary.total_records,
            records.len()
        ));
    }
    for (index, (left, right)) in records.iter().zip(decoded.records.iter()).enumerate() {
        if left.record_type() != right.record_type() {
            return Err(format!(
                "record {index} type {:?} != {:?}",
                left.record_type(),
                right.record_type()
            ));
        }
        if format!("{left:?}") != format!("{right:?}") {
            return Err(format!(
                "record {index} fields differ between decode entry points"
            ));
        }
    }
    if error.is_some() != decoded.error.is_some() {
        return Err(
            "decode_all and decode_with_summary disagree on whether decoding stopped".into(),
        );
    }
    if summary.truncated != decoded.summary.truncated {
        return Err("summarize and decode_with_summary disagree on truncation".into());
    }
    if summary.bytes_consumed != decoded.summary.bytes_consumed {
        return Err(format!(
            "bytes consumed {} != {}",
            summary.bytes_consumed, decoded.summary.bytes_consumed
        ));
    }
    if summary.byte_order != decoded.summary.byte_order {
        return Err("byte order mismatch between summarize and decode_with_summary".into());
    }
    if summary.bytes_consumed > data.len() {
        return Err("bytes consumed exceeds the input".into());
    }
    Ok(())
}

/// Records decoded from a prefix of `data` are a prefix of the records decoded
/// from all of `data`. Inputs longer than 96 bytes are sampled at a few cuts
/// so a fuzzer stays bounded; shorter inputs check every cut.
pub fn prefix_stable(data: &[u8]) -> std::result::Result<(), String> {
    let (full, _) = decode_all(data);
    let cuts: Vec<usize> = if data.len() <= 96 {
        (0..=data.len()).collect()
    } else {
        sampled_cuts(data)
    };
    for cut in cuts {
        let (partial, _) = decode_all(&data[..cut]);
        if partial.len() > full.len() {
            return Err(format!(
                "prefix of {cut} bytes decoded {} records, more than the full {}",
                partial.len(),
                full.len()
            ));
        }
        for (index, record) in partial.iter().enumerate() {
            if !same_record(record, &full[index]) {
                return Err(format!(
                    "prefix of {cut} bytes changed record {index} ({:?})",
                    record.record_type()
                ));
            }
        }
        decoding_invariants(&data[..cut])?;
    }
    decoding_invariants(data)
}

fn sampled_cuts(data: &[u8]) -> Vec<usize> {
    let mut cuts = vec![
        0,
        1.min(data.len()),
        6.min(data.len()),
        data.len() / 2,
        data.len(),
    ];
    if let Some(byte) = data.first() {
        cuts.push((*byte as usize) % (data.len() + 1));
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

fn same_record(left: &StdfRecord, right: &StdfRecord) -> bool {
    left.record_type() == right.record_type() && format!("{left:?}") == format!("{right:?}")
}
