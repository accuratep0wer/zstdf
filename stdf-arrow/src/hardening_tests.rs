use arrow::array::{Array, BooleanArray, StringArray, UInt32Array};
use stdf_core::records::{Mir, Pir, Prr, Ptr, Wir};
use stdf_core::{StdfError, StdfRecord};

use crate::schema::{PART_ID, PART_PASS, TEST_NUM, TEST_PASS, WAFER_ID};
use crate::{records_to_batches, BatchBuilder};

#[test]
fn interleaved_multisite_parts_keep_tests_with_correct_sites() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_MULTI"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Pir(pir(1, 1))),
        Ok(StdfRecord::Ptr(ptr(100, 1, 0, 1.0, 0))),
        Ok(StdfRecord::Ptr(ptr(200, 1, 1, 2.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 1, Some("SITE1"), 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("SITE0"), 0))),
    ];

    let batches = records_to_batches(records, 10).unwrap();
    let batch = &batches[0];

    assert_eq!(batch.num_rows(), 2);
    assert_eq!(as_string(batch, PART_ID).value(0), "SITE1");
    assert_eq!(as_u32(batch, TEST_NUM).value(0), 200);
    assert_eq!(as_string(batch, PART_ID).value(1), "SITE0");
    assert_eq!(as_u32(batch, TEST_NUM).value(1), 100);
}

#[test]
fn coordinate_fallback_is_attempt_local_when_sites_are_interleaved() {
    let coordinate = |num, site, name: &str, value| {
        let mut result = ptr(num, 1, site, value, 0);
        result.test_txt = Some(name.into());
        StdfRecord::Ptr(result)
    };
    let mut records = vec![
        StdfRecord::Mir(mir("LOT1")),
        StdfRecord::Wir(wir("bad-wafer")),
        StdfRecord::Pir(pir(1, 0)),
        StdfRecord::Pir(pir(1, 1)),
        coordinate(1, 0, "coordinate_x", 10.0),
        coordinate(1, 1, "coordinate_x", 11.0),
        coordinate(2, 1, "Y_INDEX", 21.0),
        coordinate(2, 0, "Y_INDEX", 20.0),
        StdfRecord::Prr(prr(1, 1, Some("SAME"), 0)),
        StdfRecord::Prr(prr(1, 0, Some("SAME"), 0)),
    ];
    records.extend([
        StdfRecord::Pir(pir(1, 0)),
        coordinate(1, 0, "coordinate_x", 12.0),
        coordinate(2, 0, "Y_INDEX", 20.0),
        StdfRecord::Prr(prr(1, 0, Some("SAME"), 0)),
    ]);
    // Both conversion paths must resolve the same identities, even though
    // the bounded path emits one part per batch in PRR completion order.
    let ordinary = records_to_batches(records.clone().into_iter().map(Ok), 100).unwrap();
    let bounded: Vec<_> = crate::bounded_record_batches(
        records.into_iter().map(Ok),
        crate::BatchLimits {
            max_pending_tests: 100,
            max_memory_bytes: 1024 * 1024,
        },
    )
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap();
    for batches in [ordinary, bounded] {
        let mut actual = Vec::new();
        for batch in batches {
            let keys = as_string(&batch, crate::schema::PART_MERGE_KEY);
            let sequences = batch
                .column(crate::schema::PART_SEQUENCE)
                .as_any()
                .downcast_ref::<arrow::array::UInt64Array>()
                .unwrap();
            for index in 0..batch.num_rows() {
                actual.push((keys.value(index).to_string(), sequences.value(index)));
            }
        }
        let expected: Vec<_> = [
            (r#"["lot-ptr","LOT1",11,21]"#, 2),
            (r#"["lot-ptr","LOT1",10,20]"#, 1),
            (r#"["lot-ptr","LOT1",12,20]"#, 3),
        ]
        .into_iter()
        .flat_map(|(key, sequence)| {
            let item = (key.to_string(), sequence);
            [item.clone(), item]
        })
        .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn open_parts_keep_starting_lot_and_wafer_across_context_changes() {
    let records = vec![
        StdfRecord::Mir(mir("L1")),
        StdfRecord::Wir(wir("W1")),
        StdfRecord::Pir(pir(1, 0)),
        StdfRecord::Ptr(ptr(1, 1, 0, 1.0, 0)),
        StdfRecord::Mir(mir("L2")),
        StdfRecord::Wir(wir("W2")),
        StdfRecord::Pir(pir(1, 1)),
        StdfRecord::Ptr(ptr(2, 1, 1, 2.0, 0)),
        StdfRecord::Prr(prr(1, 1, Some("B"), 0)),
        StdfRecord::Prr(prr(1, 0, Some("A"), 0)),
    ];
    let batch = records_to_batches(records.into_iter().map(Ok), 100)
        .unwrap()
        .remove(0);
    assert_eq!(as_string(&batch, WAFER_ID).value(0), "W2");
    assert_eq!(as_string(&batch, WAFER_ID).value(1), "W1");
    assert_eq!(as_string(&batch, 0).value(1), "L1");
}

#[test]
fn bounded_batches_reject_pending_tests_and_incomplete_parts() {
    let limits = crate::BatchLimits {
        max_pending_tests: 1,
        max_memory_bytes: 1024 * 1024,
    };
    let records = vec![
        StdfRecord::Pir(pir(1, 0)),
        StdfRecord::Ptr(ptr(1, 1, 0, 1.0, 0)),
        StdfRecord::Ptr(ptr(2, 1, 0, 1.0, 0)),
    ];
    let mut batches = crate::bounded_record_batches(records.into_iter().map(Ok), limits).unwrap();
    assert!(matches!(
        batches.next().unwrap(),
        Err(StdfError::ResourceLimit {
            resource: "pending tests",
            ..
        })
    ));
    assert!(batches.next().is_none());
    let mut batches =
        crate::bounded_record_batches(vec![Ok(StdfRecord::Pir(pir(1, 0)))], limits).unwrap();
    assert!(batches
        .next()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("incomplete"));
}

#[test]
fn bounded_batches_account_for_tests_across_sites() {
    let limits = crate::BatchLimits {
        max_pending_tests: 1,
        max_memory_bytes: 1024 * 1024,
    };
    let records = vec![
        StdfRecord::Ptr(ptr(1, 1, 0, 1.0, 0)),
        StdfRecord::Ptr(ptr(2, 1, 1, 1.0, 0)),
    ];
    assert!(
        crate::bounded_record_batches(records.into_iter().map(Ok), limits)
            .unwrap()
            .next()
            .unwrap()
            .is_err()
    );
}

#[test]
fn bounded_batches_reject_tiny_memory_and_duplicate_pir() {
    let records = vec![StdfRecord::Pir(pir(1, 0)), StdfRecord::Pir(pir(1, 0))];
    let limits = crate::BatchLimits {
        max_pending_tests: 100,
        max_memory_bytes: 1024 * 1024,
    };
    assert!(
        crate::bounded_record_batches(records.clone().into_iter().map(Ok), limits)
            .unwrap()
            .next()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("duplicate PIR")
    );
    let limits = crate::BatchLimits {
        max_memory_bytes: 1,
        ..limits
    };
    assert!(crate::bounded_record_batches(records.into_iter().map(Ok), limits).is_err());
}

#[test]
fn ptr_without_pir_synthesizes_stable_part_id() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_SYNTH"))),
        Ok(StdfRecord::Ptr(ptr(300, 1, 0, 3.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, None, 0))),
    ];

    let batches = records_to_batches(records, 10).unwrap();

    assert_eq!(as_string(&batches[0], PART_ID).value(0), "H1_S0_P1");
}

#[test]
fn prr_without_tests_finishes_zero_row_batch() {
    let mut builder = BatchBuilder::new(10);
    builder.push_record(&StdfRecord::Mir(mir("LOT_EMPTY_PART")));
    builder.push_record(&StdfRecord::Prr(prr(1, 0, Some("P0"), 0)));

    let batch = builder.finish().unwrap();

    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.num_columns(), 21);
}

#[test]
fn wafer_id_updates_between_completed_parts() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_WAFER"))),
        Ok(StdfRecord::Wir(wir("W1"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(401, 1, 0, 4.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("P1"), 0))),
        Ok(StdfRecord::Wir(wir("W2"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(402, 1, 0, 5.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("P2"), 0))),
    ];

    let batches = records_to_batches(records, 10).unwrap();
    let wafer_id = as_string(&batches[0], WAFER_ID);

    assert_eq!(wafer_id.value(0), "W1");
    assert_eq!(wafer_id.value(1), "W2");
}

#[test]
fn failing_part_flag_propagates_to_part_pass_column() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_FAIL"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(501, 1, 0, 5.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("PF"), 0x08))),
    ];

    let batches = records_to_batches(records, 10).unwrap();

    assert!(!as_bool(&batches[0], PART_PASS).value(0));
}

#[test]
fn zero_batch_size_is_treated_as_one() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_ZERO_BATCH"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(601, 1, 0, 6.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("P1"), 0))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(602, 1, 0, 7.0, 0))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("P2"), 0))),
    ];

    let batches = records_to_batches(records, 0).unwrap();

    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].num_rows(), 1);
    assert_eq!(batches[1].num_rows(), 1);
}

#[test]
fn invalid_test_pass_flag_maps_to_null() {
    let records = vec![
        Ok(StdfRecord::Mir(mir("LOT_FLAG"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
        Ok(StdfRecord::Ptr(ptr(701, 1, 0, 7.0, 0x40))),
        Ok(StdfRecord::Prr(prr(1, 0, Some("PFLAG"), 0))),
    ];

    let batches = records_to_batches(records, 10).unwrap();

    assert!(as_bool(&batches[0], TEST_PASS).is_null(0));
}

#[test]
fn records_to_batches_propagates_decode_errors() {
    let result = records_to_batches(vec![Err(StdfError::UnsupportedVersion(3))], 10);

    assert!(matches!(result, Err(StdfError::UnsupportedVersion(3))));
}

#[test]
fn many_ptrs_for_one_part_remain_in_order() {
    let mut records = vec![
        Ok(StdfRecord::Mir(mir("LOT_MANY"))),
        Ok(StdfRecord::Pir(pir(1, 0))),
    ];
    for test_num in 800..850 {
        records.push(Ok(StdfRecord::Ptr(ptr(test_num, 1, 0, test_num as f32, 0))));
    }
    records.push(Ok(StdfRecord::Prr(prr(1, 0, Some("PMANY"), 0))));

    let batches = records_to_batches(records, 100).unwrap();
    let test_num = as_u32(&batches[0], TEST_NUM);

    assert_eq!(batches[0].num_rows(), 50);
    assert_eq!(test_num.value(0), 800);
    assert_eq!(test_num.value(49), 849);
}

fn as_string(batch: &arrow::record_batch::RecordBatch, index: usize) -> &StringArray {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
}

fn as_u32(batch: &arrow::record_batch::RecordBatch, index: usize) -> &UInt32Array {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .unwrap()
}

fn as_bool(batch: &arrow::record_batch::RecordBatch, index: usize) -> &BooleanArray {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<BooleanArray>()
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

fn ptr(test_num: u32, head_num: u8, site_num: u8, result: f32, test_flg: u8) -> Ptr {
    Ptr {
        test_num,
        head_num,
        site_num,
        test_flg,
        parm_flg: 0,
        result,
        test_txt: Some(format!("T{test_num}")),
        alarm_id: None,
        opt_flag: None,
        res_scal: None,
        llm_scal: None,
        hlm_scal: None,
        lo_limit: None,
        hi_limit: None,
        units: None,
        c_resfmt: None,
        c_llmfmt: None,
        c_hlmfmt: None,
        lo_spec: None,
        hi_spec: None,
    }
}

fn prr(head_num: u8, site_num: u8, part_id: Option<&str>, part_flg: u8) -> Prr {
    Prr {
        head_num,
        site_num,
        part_flg,
        num_test: 1,
        hard_bin: 1,
        soft_bin: 1,
        x_coord: None,
        y_coord: None,
        test_t: None,
        part_id: part_id.map(str::to_string),
        part_txt: None,
        part_fix: None,
    }
}

fn mpr_fixture(site: u8, flags: u8, values: &[f32]) -> stdf_core::records::Mpr {
    let mut body = 700u32.to_le_bytes().to_vec();
    body.extend([1, site, flags, 0]);
    body.extend(0u16.to_le_bytes());
    body.extend((values.len() as u16).to_le_bytes());
    for value in values {
        body.extend(value.to_le_bytes());
    }
    stdf_core::records::Mpr::parse(&mut stdf_core::fields::FieldReader::new(
        &body,
        stdf_core::ByteOrder::LittleEndian,
    ))
    .unwrap()
}

fn ftr_fixture(site: u8, flags: u8) -> stdf_core::records::Ftr {
    let mut body = 700u32.to_le_bytes().to_vec();
    body.extend([1, site, flags]);
    stdf_core::records::Ftr::parse(&mut stdf_core::fields::FieldReader::new(
        &body,
        stdf_core::ByteOrder::LittleEndian,
    ))
    .unwrap()
}

#[test]
fn mpr_and_ftr_expand_on_both_paths_without_cross_site_leakage() {
    let records = vec![
        StdfRecord::Pir(pir(1, 0)),
        StdfRecord::Pir(pir(1, 1)),
        StdfRecord::Mpr(mpr_fixture(0, 128, &[1.0, 2.0])),
        StdfRecord::Ftr(ftr_fixture(1, 0)),
        StdfRecord::Prr(prr(1, 1, Some("SAME"), 0)),
        StdfRecord::Prr(prr(1, 0, Some("SAME"), 8)),
    ];
    let ordinary = records_to_batches(records.clone().into_iter().map(Ok), 1).unwrap();
    let bounded = crate::bounded_record_batches(
        records.into_iter().map(Ok),
        crate::BatchLimits {
            max_pending_tests: 4,
            max_memory_bytes: 1024 * 1024,
        },
    )
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(ordinary, bounded);
    assert_eq!(bounded.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
    assert!(bounded[0].column(crate::schema::RESULT).is_null(0));
    assert!(as_bool(&bounded[0], crate::schema::TEST_PASS).value(0));
    let batch = &bounded[1];
    assert_eq!(as_string(batch, crate::schema::TEST_TYPE).value(0), "MPR");
    assert!(!as_bool(batch, crate::schema::TEST_PASS).value(0));
    assert!(batch.column(crate::schema::TEST_PASS).is_null(1));
    assert!(batch.column(crate::schema::TEST_PASS).is_null(2));
    assert_eq!(as_f32(batch, crate::schema::RESULT).value(1), 1.0);
    assert_eq!(as_f32(batch, crate::schema::RESULT).value(2), 2.0);
    assert!(as_string(batch, crate::schema::TEST_TXT)
        .value(2)
        .ends_with("[MPR result 1]"));
}

#[test]
fn mpr_budget_counts_every_emitted_row_and_releases_completed_parts() {
    let limits = crate::BatchLimits {
        max_pending_tests: 3,
        max_memory_bytes: 1024 * 1024,
    };
    let records = vec![
        StdfRecord::Mpr(mpr_fixture(0, 0, &[1.0, 2.0])),
        StdfRecord::Ftr(ftr_fixture(1, 0)),
    ];
    let error = crate::bounded_record_batches(records.into_iter().map(Ok), limits)
        .unwrap()
        .next()
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        error,
        StdfError::ResourceLimit {
            resource: "pending tests",
            ..
        }
    ));
    let records = vec![
        StdfRecord::Mpr(mpr_fixture(0, 0, &[1.0, 2.0])),
        StdfRecord::Prr(prr(1, 0, None, 0)),
        StdfRecord::Mpr(mpr_fixture(1, 0, &[3.0, 4.0])),
        StdfRecord::Prr(prr(1, 1, None, 0)),
    ];
    assert_eq!(
        crate::bounded_record_batches(records.into_iter().map(Ok), limits)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len(),
        2
    );
    let memory = crate::BatchLimits {
        max_pending_tests: 1000,
        max_memory_bytes: 140_000,
    };
    let records = vec![Ok(StdfRecord::Mpr(mpr_fixture(0, 0, &[1.0; 20])))];
    assert!(matches!(
        crate::bounded_record_batches(records, memory)
            .unwrap()
            .next()
            .unwrap(),
        Err(StdfError::ResourceLimit {
            resource: "pending bytes",
            ..
        })
    ));
}

#[test]
fn missing_mpr_arrays_fail_and_zero_results_retain_overall_verdict() {
    let mut broken = mpr_fixture(0, 0, &[1.0, 2.0]);
    broken.rtn_rslt = None;
    assert!(records_to_batches(vec![Ok(StdfRecord::Mpr(broken.clone()))], 100).is_err());
    let records = vec![Ok(StdfRecord::Mpr(broken))];
    assert!(crate::bounded_record_batches(
        records,
        crate::BatchLimits {
            max_pending_tests: 100,
            max_memory_bytes: 1024 * 1024
        }
    )
    .unwrap()
    .next()
    .unwrap()
    .is_err());
    for flags in [0, 128, 2, 4, 8, 16, 32, 64] {
        let rows = records_to_batches(
            vec![
                Ok(StdfRecord::Mpr(mpr_fixture(0, flags, &[]))),
                Ok(StdfRecord::Ftr(ftr_fixture(0, flags))),
                Ok(StdfRecord::Prr(prr(1, 0, None, 0))),
            ],
            100,
        )
        .unwrap();
        assert_eq!(rows[0].num_rows(), 2);
        for row in 0..2 {
            assert_eq!(
                rows[0].column(crate::schema::TEST_PASS).is_null(row),
                flags & 0x7e != 0
            );
        }
    }
}

fn as_f32(batch: &arrow::record_batch::RecordBatch, index: usize) -> &arrow::array::Float32Array {
    batch.column(index).as_any().downcast_ref().unwrap()
}
