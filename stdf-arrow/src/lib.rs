// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

pub mod batch_builder;
mod bounded;
pub mod context;
pub mod identity;
pub mod schema;
pub use bounded::{bounded_record_batches, BatchLimits, BoundedRecordBatchIter};

use arrow::record_batch::RecordBatch;
use stdf_core::{StdfError, StdfRecord};

pub use batch_builder::BatchBuilder;
pub use context::{PartResult, StdfContext, TestResult};
pub use schema::eav_schema;

pub fn records_to_batches(
    records: impl IntoIterator<Item = std::result::Result<StdfRecord, StdfError>>,
    batch_size: usize,
) -> std::result::Result<Vec<RecordBatch>, StdfError> {
    let mut builder = BatchBuilder::new(batch_size);
    let mut batches = Vec::new();

    for record in records {
        if let Some(batch) = builder.push_record(&record?) {
            batches.push(batch);
        }
    }
    if let Some(batch) = builder.finish() {
        batches.push(batch);
    }

    Ok(batches)
}

pub fn record_batches<I>(records: I, batch_size: usize) -> RecordBatchIter<I::IntoIter>
where
    I: IntoIterator<Item = std::result::Result<StdfRecord, StdfError>>,
{
    RecordBatchIter {
        records: records.into_iter(),
        builder: BatchBuilder::new(batch_size),
        finished: false,
    }
}

pub struct RecordBatchIter<I> {
    records: I,
    builder: BatchBuilder,
    finished: bool,
}

impl<I> Iterator for RecordBatchIter<I>
where
    I: Iterator<Item = std::result::Result<StdfRecord, StdfError>>,
{
    type Item = std::result::Result<RecordBatch, StdfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        for record in self.records.by_ref() {
            match record {
                Ok(record) => {
                    if let Some(batch) = self.builder.push_record(&record) {
                        return Some(Ok(batch));
                    }
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            }
        }

        self.finished = true;
        std::mem::replace(&mut self.builder, BatchBuilder::new(1))
            .finish()
            .map(Ok)
    }
}

#[cfg(test)]
mod hardening_tests;

#[cfg(test)]
mod tests {
    use arrow::array::{Array, BooleanArray, Float32Array, Int16Array, StringArray, UInt32Array};
    use stdf_core::records::{Mir, Pir, Prr, Ptr, Wir};

    use super::*;
    use crate::schema::{
        HI_LIMIT, LOT_ID, PART_ID, RESULT, TEST_NUM, TEST_PASS, TEST_TXT, UNITS, X_COORD,
    };

    #[test]
    fn batch_builder_emits_eav_rows_for_completed_part() {
        let mut builder = BatchBuilder::new(10);
        let records = vec![
            StdfRecord::Mir(mir("LOT001")),
            StdfRecord::Wir(wir("W01")),
            StdfRecord::Pir(pir(1, 0)),
            StdfRecord::Ptr(ptr(
                1001,
                3.14,
                Some("IDDQ"),
                Some(0.0),
                Some(5.0),
                Some("mA"),
                0x00,
            )),
            StdfRecord::Ptr(ptr(
                1002,
                5.50,
                Some("VDD"),
                Some(1.0),
                Some(4.5),
                Some("V"),
                0x80,
            )),
            StdfRecord::Prr(prr(1, 0, Some("P1"), Some(10), Some(-2), Some(123), 0x00)),
        ];

        for record in &records {
            assert!(builder.push_record(record).is_none());
        }
        let batch = builder
            .finish()
            .expect("completed part should emit a batch");

        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.schema().fields().len(), 21);

        let lot_id = as_string(&batch, LOT_ID);
        let part_id = as_string(&batch, PART_ID);
        let test_num = as_u32(&batch, TEST_NUM);
        let result = as_f32(&batch, RESULT);
        let test_pass = as_bool(&batch, TEST_PASS);
        let x_coord = as_i16(&batch, X_COORD);

        assert_eq!(lot_id.value(0), "LOT001");
        assert_eq!(part_id.value(1), "P1");
        assert_eq!(test_num.value(0), 1001);
        assert_eq!(test_num.value(1), 1002);
        assert!((result.value(0) - 3.14).abs() < 0.001);
        assert!(test_pass.value(0));
        assert!(!test_pass.value(1));
        assert_eq!(x_coord.value(0), 10);
    }

    #[test]
    fn nullable_optional_fields_remain_null() {
        let mut builder = BatchBuilder::new(10);
        let records = vec![
            StdfRecord::Mir(mir("LOT002")),
            StdfRecord::Pir(pir(1, 0)),
            StdfRecord::Ptr(ptr(2001, 1.25, None, None, None, None, 0x40)),
            StdfRecord::Prr(prr(1, 0, Some("P2"), None, None, None, 0x00)),
        ];

        for record in &records {
            builder.push_record(record);
        }
        let batch = builder
            .finish()
            .expect("completed part should emit a batch");

        assert_eq!(batch.num_rows(), 1);
        assert!(as_string(&batch, TEST_TXT).is_null(0));
        assert!(as_f32(&batch, HI_LIMIT).is_null(0));
        assert!(as_string(&batch, UNITS).is_null(0));
        assert!(as_bool(&batch, TEST_PASS).is_null(0));
        assert!(as_i16(&batch, X_COORD).is_null(0));
    }

    #[test]
    fn records_to_batches_flushes_by_completed_eav_rows() {
        let records = vec![
            Ok(StdfRecord::Mir(mir("LOT003"))),
            Ok(StdfRecord::Pir(pir(1, 0))),
            Ok(StdfRecord::Ptr(ptr(
                3001,
                1.0,
                Some("A"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Prr(prr(
                1,
                0,
                Some("P3"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Pir(pir(1, 0))),
            Ok(StdfRecord::Ptr(ptr(
                3002,
                2.0,
                Some("B"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Prr(prr(
                1,
                0,
                Some("P4"),
                None,
                None,
                None,
                0x00,
            ))),
        ];

        let batches = records_to_batches(records, 1).unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].num_rows(), 1);
        assert_eq!(as_string(&batches[1], PART_ID).value(0), "P4");
    }

    #[test]
    fn record_batches_streams_without_collecting_all_batches() {
        let records = vec![
            Ok(StdfRecord::Mir(mir("LOT004"))),
            Ok(StdfRecord::Pir(pir(1, 0))),
            Ok(StdfRecord::Ptr(ptr(
                4001,
                1.0,
                Some("A"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Prr(prr(
                1,
                0,
                Some("P5"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Pir(pir(1, 0))),
            Ok(StdfRecord::Ptr(ptr(
                4002,
                2.0,
                Some("B"),
                None,
                None,
                None,
                0x00,
            ))),
            Ok(StdfRecord::Prr(prr(
                1,
                0,
                Some("P6"),
                None,
                None,
                None,
                0x00,
            ))),
        ];

        let batches = record_batches(records, 1)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].num_rows(), 1);
        assert_eq!(as_string(&batches[1], PART_ID).value(0), "P6");
    }

    fn as_string(batch: &RecordBatch, index: usize) -> &StringArray {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
    }

    fn as_u32(batch: &RecordBatch, index: usize) -> &UInt32Array {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .unwrap()
    }

    fn as_f32(batch: &RecordBatch, index: usize) -> &Float32Array {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<Float32Array>()
            .unwrap()
    }

    fn as_bool(batch: &RecordBatch, index: usize) -> &BooleanArray {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap()
    }

    fn as_i16(batch: &RecordBatch, index: usize) -> &Int16Array {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<Int16Array>()
            .unwrap()
    }

    fn mir(lot_id: &str) -> Mir {
        Mir {
            setup_t: 0,
            start_t: 0,
            stat_num: 0,
            mode_cod: b'P',
            rtst_cod: b' ',
            prot_cod: b' ',
            burn_tim: 0,
            cmod_cod: b' ',
            lot_id: lot_id.to_string(),
            part_typ: "DEVICE".to_string(),
            node_nam: "NODE".to_string(),
            tstr_typ: "T".to_string(),
            job_nam: "JOB".to_string(),
            job_rev: None,
            sblot_id: None,
            oper_nam: None,
            exec_typ: None,
            exec_ver: None,
            test_cod: None,
            tst_temp: None,
            user_txt: None,
            aux_file: None,
            pkg_typ: None,
            famly_id: None,
            date_cod: None,
            facil_id: None,
            floor_id: None,
            proc_id: None,
            oper_frq: None,
            spec_nam: None,
            spec_ver: None,
            flow_id: None,
            setup_id: None,
            dsgn_rev: None,
            eng_id: None,
            rom_cod: None,
            serl_num: None,
            supr_nam: None,
        }
    }

    fn wir(wafer_id: &str) -> Wir {
        Wir {
            head_num: 1,
            site_grp: None,
            start_t: None,
            wafer_id: Some(wafer_id.to_string()),
        }
    }

    fn pir(head_num: u8, site_num: u8) -> Pir {
        Pir { head_num, site_num }
    }

    fn ptr(
        test_num: u32,
        result: f32,
        test_txt: Option<&str>,
        lo_limit: Option<f32>,
        hi_limit: Option<f32>,
        units: Option<&str>,
        test_flg: u8,
    ) -> Ptr {
        Ptr {
            test_num,
            head_num: 1,
            site_num: 0,
            test_flg,
            parm_flg: 0,
            result,
            test_txt: test_txt.map(str::to_string),
            alarm_id: None,
            opt_flag: None,
            res_scal: None,
            llm_scal: None,
            hlm_scal: None,
            lo_limit,
            hi_limit,
            units: units.map(str::to_string),
            c_resfmt: None,
            c_llmfmt: None,
            c_hlmfmt: None,
            lo_spec: None,
            hi_spec: None,
        }
    }

    fn prr(
        head_num: u8,
        site_num: u8,
        part_id: Option<&str>,
        x_coord: Option<i16>,
        y_coord: Option<i16>,
        test_t: Option<u32>,
        part_flg: u8,
    ) -> Prr {
        Prr {
            head_num,
            site_num,
            part_flg,
            num_test: 1,
            hard_bin: 1,
            soft_bin: 1,
            x_coord,
            y_coord,
            test_t,
            part_id: part_id.map(str::to_string),
            part_txt: None,
            part_fix: None,
        }
    }
}
