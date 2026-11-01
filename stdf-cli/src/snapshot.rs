// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Explicit historical catalog selections and source-scoped retained evidence.
use crate::telemetry::{self, TelemetryOptions};
use crate::viewer::{analysis, evidence};
use crate::CliResult;
use clap::{Args, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use stdf_parquet::snapshot::{
    create_snapshot, current_snapshot, list_snapshots, select_snapshot, verify_snapshot,
};

#[derive(Debug, Args)]
struct EvidenceBudget {
    #[arg(long, default_value_t = 256)]
    memory_limit_mib: usize,
    /// Total managed evidence storage plus operation scratch, in MiB.
    #[arg(long, default_value_t = 10240)]
    disk_limit_mib: u64,
    #[arg(long)]
    cancel_file: Option<PathBuf>,
}
impl EvidenceBudget {
    fn limits(&self) -> evidence::EvidenceLimits {
        evidence::EvidenceLimits {
            memory_limit_mib: self.memory_limit_mib,
            disk_limit_mib: self.disk_limit_mib,
            cancel_file: self.cancel_file.clone(),
        }
    }
}

#[derive(Debug, Args)]
pub struct Arguments {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Retain native viewer evidence for one explicit frozen snapshot source.
    RetainEvidence {
        dataset: PathBuf,
        snapshot_id: String,
        source_id: String,
        input: PathBuf,
        #[command(flatten)]
        budget: EvidenceBudget,
    },
    /// Verify retained evidence using its decompressed source and bounded reindexing.
    VerifyEvidence {
        dataset: PathBuf,
        evidence_id: String,
        #[command(flatten)]
        budget: EvidenceBudget,
    },
    /// List retained evidence directory IDs without asserting integrity.
    ListEvidence { dataset: PathBuf },
    /// Read one verified original record, including raw and resolved fields.
    EvidenceRecord {
        dataset: PathBuf,
        evidence_id: String,
        record_id: u64,
        #[command(flatten)]
        budget: EvidenceBudget,
    },
    /// Read one verified PRR attempt, including attempts without measurements.
    EvidenceAttempt {
        dataset: PathBuf,
        evidence_id: String,
        attempt_id: u64,
        #[command(flatten)]
        budget: EvidenceBudget,
    },
    /// Run `attempt-yield-request-v1` over retained evidence for a frozen snapshot.
    ///
    /// Counts physical PRRs under the request population (not Dashboard merge).
    AttemptYield {
        dataset: PathBuf,
        /// Path to a JSON `attempt-yield-request-v1` body.
        request: PathBuf,
        #[command(flatten)]
        budget: EvidenceBudget,
        /// Append local run-telemetry JSONL events; omit to preserve result-only stdout.
        #[arg(long)]
        telemetry_json: Option<PathBuf>,
    },
    /// Pin the verified current EAV catalog; expected input completeness stays unknown.
    Create {
        input: PathBuf,
        /// Explicitly pin a partial, failed or interrupted catalog; never a running one.
        #[arg(long)]
        allow_incomplete: bool,
        /// Select this snapshot as current after creation and verification.
        #[arg(long)]
        select: bool,
    },
    /// Verify and explicitly select a retained snapshot as current.
    Select {
        input: PathBuf,
        id: String,
        /// Allow a saved snapshot with an incomplete import status or recorded failures.
        #[arg(long)]
        allow_incomplete: bool,
    },
    /// Resolve the current snapshot once and verify it; never fall back to the live catalog.
    Current { input: PathBuf },
    /// List retained snapshot IDs; this does not verify fragment integrity.
    List { input: PathBuf },
    /// Verify a retained snapshot and its EAV fragments without using the current catalog.
    Verify { input: PathBuf, id: String },
}

pub fn execute(args: Arguments, out: &mut impl Write) -> CliResult<()> {
    let value = match args.action {
        Action::RetainEvidence {
            dataset,
            snapshot_id,
            source_id,
            input,
            budget,
        } => serde_json::to_value(evidence::retain(
            &dataset,
            &snapshot_id,
            &source_id,
            &input,
            &budget.limits(),
        )?)?,
        Action::VerifyEvidence {
            dataset,
            evidence_id,
            budget,
        } => serde_json::to_value(evidence::verify(&dataset, &evidence_id, &budget.limits())?)?,
        Action::ListEvidence { dataset } => {
            serde_json::json!({"evidence_ids":evidence::list(&dataset)?,
            "verification":"directory names only; use snapshot verify-evidence for integrity"})
        }
        Action::EvidenceRecord {
            dataset,
            evidence_id,
            record_id,
            budget,
        } => evidence::record(&dataset, &evidence_id, record_id, &budget.limits())?,
        Action::EvidenceAttempt {
            dataset,
            evidence_id,
            attempt_id,
            budget,
        } => evidence::attempt(&dataset, &evidence_id, attempt_id, &budget.limits())?,
        Action::AttemptYield {
            dataset,
            request,
            budget,
            telemetry_json,
        } => {
            let options = TelemetryOptions {
                json_path: telemetry_json,
            };
            let mut telemetry = telemetry::RunTelemetry::start("attempt-yield", &options);
            analysis::execute_with_telemetry(
                &dataset,
                &request,
                &budget.limits(),
                Some(&mut telemetry),
            )?
        }
        Action::Create {
            input,
            allow_incomplete,
            select,
        } => {
            let snapshot = create_snapshot(&input, allow_incomplete)?;
            if select {
                select_snapshot(&input, &snapshot.snapshot_id, allow_incomplete).map_err(
                    |error| {
                        format!(
                            "snapshot {} was created, but selecting it failed: {error}",
                            snapshot.snapshot_id
                        )
                    },
                )?;
            }
            serde_json::to_value(snapshot)?
        }
        Action::Select {
            input,
            id,
            allow_incomplete,
        } => serde_json::to_value(select_snapshot(&input, &id, allow_incomplete)?)?,
        Action::Current { input } => serde_json::to_value(current_snapshot(&input)?)?,
        Action::Verify { input, id } => serde_json::to_value(verify_snapshot(&input, &id)?)?,
        Action::List { input } => serde_json::json!({
            "snapshot_ids": list_snapshots(&input)?,
            "verification": "manifest names only; use snapshot verify for integrity"
        }),
    };
    serde_json::to_writer_pretty(&mut *out, &value)?;
    writeln!(out)?;
    Ok(())
}
