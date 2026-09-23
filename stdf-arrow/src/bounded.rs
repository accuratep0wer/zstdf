use crate::StdfContext;
use arrow::record_batch::RecordBatch;
use std::collections::{BTreeMap, BTreeSet};
use stdf_core::{StdfError, StdfRecord};

#[derive(Debug, Clone, Copy)]
pub struct BatchLimits {
    pub max_pending_tests: usize,
    pub max_memory_bytes: usize,
}

/// Emit one completed part per batch. Limits are checked before retaining a test.
/// Byte charges reserve space for pending results and the eventual Arrow copy;
/// they are conservative accounting, not a limit on process RSS/allocator overhead.
pub fn bounded_record_batches<I>(
    records: I,
    limits: BatchLimits,
) -> Result<BoundedRecordBatchIter<I::IntoIter>, StdfError>
where
    I: IntoIterator<Item = Result<StdfRecord, StdfError>>,
{
    if limits.max_pending_tests == 0 || limits.max_memory_bytes == 0 {
        return Err(invalid("limits must be positive"));
    }
    check("context reserve bytes", 128 * 1024, limits.max_memory_bytes)?;
    Ok(BoundedRecordBatchIter {
        records: records.into_iter(),
        context: StdfContext::new(),
        pending: BTreeMap::new(),
        metadata_keys: BTreeSet::new(),
        tests: 0,
        bytes: 128 * 1024,
        peak_bytes: 128 * 1024,
        limits,
        finished: false,
        provenance: None,
        record_sequence: 0,
        last_provenance: Vec::new(),
    })
}

pub struct BoundedRecordBatchIter<I> {
    records: I,
    context: StdfContext,
    pending: BTreeMap<(u8, u8), (usize, usize)>,
    metadata_keys: BTreeSet<(u8, u8, u8)>,
    tests: usize,
    bytes: usize,
    peak_bytes: usize,
    limits: BatchLimits,
    finished: bool,
    provenance: Option<BTreeMap<(u8, u8), Vec<RowProvenance>>>,
    record_sequence: u64,
    last_provenance: Vec<RowProvenance>,
}

/// Exact source record ordinal and expansion index for an emitted EAV row.
/// MPR index 0 is its overall verdict; indices 1.. are RTN_RSLT values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowProvenance {
    pub record_sequence: u64,
    pub expansion_index: u32,
}

impl<I> BoundedRecordBatchIter<I> {
    /// Enable before reading. Existing converters incur no provenance allocation.
    pub fn with_provenance(mut self) -> Self {
        assert_eq!(self.record_sequence, 0, "enable provenance before reading");
        self.provenance = Some(BTreeMap::new());
        self
    }

    pub fn last_provenance(&self) -> &[RowProvenance] {
        &self.last_provenance
    }
    pub fn peak_reserved_bytes(&self) -> usize {
        self.peak_bytes
    }

    fn metadata(&mut self, key: (u8, u8, u8)) -> Result<(), StdfError> {
        if !self.metadata_keys.contains(&key) {
            check(
                "context metadata bytes",
                self.bytes.saturating_add(2048),
                self.limits.max_memory_bytes,
            )?;
            self.metadata_keys.insert(key);
            self.bytes += 2048;
            self.peak_bytes = self.peak_bytes.max(self.bytes);
        }
        Ok(())
    }

    fn retain(&mut self, key: (u8, u8), count: usize, test_bytes: usize) -> Result<(), StdfError> {
        let new_part = !self.pending.contains_key(&key);
        let charge = test_bytes.saturating_add(if new_part { 4096 } else { 0 });
        let tests = self.tests.saturating_add(count);
        check("pending tests", tests, self.limits.max_pending_tests)?;
        check(
            "pending bytes",
            self.bytes.saturating_add(charge),
            self.limits.max_memory_bytes,
        )?;
        let entry = self.pending.entry(key).or_default();
        entry.0 += count;
        entry.1 += charge;
        self.tests = tests;
        self.bytes += charge;
        self.peak_bytes = self.peak_bytes.max(self.bytes);
        Ok(())
    }

    fn push(&mut self, record: &StdfRecord) -> Result<Option<RecordBatch>, StdfError> {
        self.record_sequence += 1;
        let context_string_len = match record {
            StdfRecord::Mir(mir) => mir.lot_id.len(),
            StdfRecord::Wir(wir) => wir.wafer_id.as_ref().map_or(0, String::len),
            StdfRecord::Prr(prr) => prr.part_id.as_ref().map_or(0, String::len),
            _ => 0,
        };
        check("STDF context string bytes", context_string_len, 255)?;
        match record {
            StdfRecord::Wir(wir) => {
                self.metadata((0, wir.head_num, wir.site_grp.unwrap_or(255)))?
            }
            StdfRecord::Sdr(sdr) => {
                for site in &sdr.site_num {
                    self.metadata((1, sdr.head_num, *site))?;
                }
            }
            StdfRecord::Pir(pir) => {
                let key = (pir.head_num, pir.site_num);
                if self.pending.contains_key(&key) {
                    return Err(invalid("duplicate PIR would discard an incomplete part"));
                }
                self.retain(key, 0, 0)?;
            }
            StdfRecord::Ptr(ptr) => {
                let strings = ptr
                    .test_txt
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(ptr.units.as_ref().map_or(0, String::len));
                self.retain(
                    (ptr.head_num, ptr.site_num),
                    1,
                    4096usize.saturating_add(strings.saturating_mul(4)),
                )?;
            }
            StdfRecord::Mpr(mpr) => {
                let count = usize::from(mpr.rslt_cnt) + 1;
                check(
                    "pending tests",
                    self.tests.saturating_add(count),
                    self.limits.max_pending_tests,
                )?;
                crate::context::validate_expansion(record)?;
                let strings = mpr
                    .test_txt
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(mpr.units.as_ref().map_or(0, String::len));
                self.retain(
                    (mpr.head_num, mpr.site_num),
                    count,
                    count.saturating_mul(4096usize.saturating_add(strings.saturating_mul(4))),
                )?;
            }
            StdfRecord::Ftr(ftr) => {
                self.retain(
                    (ftr.head_num, ftr.site_num),
                    1,
                    4096usize.saturating_add(
                        ftr.test_txt
                            .as_ref()
                            .map_or(0, String::len)
                            .saturating_mul(4),
                    ),
                )?;
            }
            StdfRecord::Unknown { typ, sub, .. }
                if matches!(
                    (*typ, *sub),
                    (1, 10)
                        | (1, 80)
                        | (2, 10)
                        | (5, 10)
                        | (5, 20)
                        | (15, 10)
                        | (15, 15)
                        | (15, 20)
                ) =>
            {
                return Err(invalid("malformed metadata/part/test record"))
            }
            _ => {}
        }
        if let Some(origins) = &mut self.provenance {
            let test = match record {
                StdfRecord::Ptr(r) => Some(((r.head_num, r.site_num), 1)),
                StdfRecord::Ftr(r) => Some(((r.head_num, r.site_num), 1)),
                StdfRecord::Mpr(r) => Some(((r.head_num, r.site_num), u32::from(r.rslt_cnt) + 1)),
                _ => None,
            };
            // The existing per-test 4096-byte reservation covers these 16-byte entries.
            if let Some((site, count)) = test {
                origins
                    .entry(site)
                    .or_default()
                    .extend((0..count).map(|i| RowProvenance {
                        record_sequence: self.record_sequence,
                        expansion_index: i,
                    }));
            }
        }
        if let Some(part) = self.context.push_record(record) {
            let key = (part.head_num, part.site_num);
            self.last_provenance = self
                .provenance
                .as_mut()
                .and_then(|p| p.remove(&key))
                .unwrap_or_default();
            let batch = crate::batch_builder::build_batch(std::slice::from_ref(&part));
            if let Some((tests, bytes)) = self.pending.remove(&key) {
                self.tests -= tests;
                self.bytes -= bytes;
            }
            return Ok((batch.num_rows() > 0).then_some(batch));
        }
        Ok(None)
    }
}

impl<I: Iterator<Item = Result<StdfRecord, StdfError>>> Iterator for BoundedRecordBatchIter<I> {
    type Item = Result<RecordBatch, StdfError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            match self.records.next() {
                Some(record) => match record.and_then(|record| self.push(&record)) {
                    Ok(Some(batch)) => return Some(Ok(batch)),
                    Ok(None) => {}
                    Err(error) => {
                        self.finished = true;
                        self.context = StdfContext::new();
                        self.pending.clear();
                        return Some(Err(error));
                    }
                },
                None => {
                    self.finished = true;
                    let incomplete = !self.pending.is_empty();
                    self.context = StdfContext::new();
                    self.pending.clear();
                    return incomplete
                        .then(|| Err(invalid("incomplete parts at end of input: missing PRR")));
                }
            }
        }
    }
}

pub(crate) fn check(
    resource: &'static str,
    requested: usize,
    limit: usize,
) -> Result<(), StdfError> {
    if requested > limit {
        Err(StdfError::ResourceLimit {
            resource,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}
fn invalid(message: &str) -> StdfError {
    StdfError::InvalidField {
        record: "EAV",
        field: "stream",
        msg: message.into(),
    }
}
