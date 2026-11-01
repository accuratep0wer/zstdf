// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Local-first run telemetry for convert, view ingest and analysis.
//!
//! Emits stderr one-liners and optionally appends JSONL. This is not OpenTelemetry
//! and does not change command results when `--telemetry-json` is omitted.

use serde_json::{json, Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Optional JSONL destination; omit to keep prior CLI behavior aside from stderr phase lines.
#[derive(Clone, Debug, Default)]
pub struct TelemetryOptions {
    pub json_path: Option<PathBuf>,
}

/// Phase/counter/budget recorder for one CLI operation.
pub struct RunTelemetry {
    command: String,
    started: Instant,
    phase_started: Instant,
    phase: String,
    counters: Map<String, Value>,
    budgets: Map<String, Value>,
    json_path: Option<PathBuf>,
    terminal: Option<&'static str>,
}

impl RunTelemetry {
    pub fn start(command: impl Into<String>, options: &TelemetryOptions) -> Self {
        let now = Instant::now();
        Self {
            command: command.into(),
            started: now,
            phase_started: now,
            phase: "start".into(),
            counters: Map::new(),
            budgets: Map::new(),
            json_path: options.json_path.clone(),
            terminal: None,
        }
    }

    pub fn phase(&mut self, name: impl Into<String>) {
        self.emit_phase(None);
        self.phase = name.into();
        self.phase_started = Instant::now();
    }

    pub fn counter(&mut self, name: impl Into<String>, value: u64) {
        self.counters.insert(name.into(), json!(value));
    }

    pub fn budget(&mut self, name: impl Into<String>, used: u64, limit: u64) {
        self.budgets.insert(
            name.into(),
            json!({
                "used": used,
                "limit": limit,
            }),
        );
    }

    pub fn finish_ok(&mut self) {
        self.terminal = Some("ok");
        self.emit_phase(Some("ok"));
    }

    pub fn finish_cancelled(&mut self) {
        self.terminal = Some("cancelled");
        self.emit_phase(Some("cancelled"));
    }

    pub fn finish_quota(&mut self) {
        self.terminal = Some("quota");
        self.emit_phase(Some("quota"));
    }

    pub fn finish_error(&mut self) {
        self.terminal = Some("error");
        self.emit_phase(Some("error"));
    }

    /// Classifies a failure string into a terminal event without changing the error.
    pub fn finish_from_error(&mut self, error: &str) {
        let lower = error.to_ascii_lowercase();
        if lower.contains("cancel") {
            self.finish_cancelled();
        } else if lower.contains("budget")
            || lower.contains("quota")
            || lower.contains("limit")
            || lower.contains("exceed")
        {
            self.finish_quota();
        } else {
            self.finish_error();
        }
    }

    fn emit_phase(&mut self, terminal: Option<&str>) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        let phase_ms = self.phase_started.elapsed().as_millis() as u64;
        let status = terminal.or(self.terminal).unwrap_or("running");
        let line = format!(
            "telemetry command={} phase={} status={} elapsed_ms={} phase_ms={}",
            self.command, self.phase, status, elapsed_ms, phase_ms
        );
        eprintln!("{line}");
        if let Some(path) = &self.json_path {
            let event = json!({
                "version": "run-telemetry-v1",
                "command": self.command,
                "phase": self.phase,
                "status": status,
                "elapsed_ms": elapsed_ms,
                "phase_ms": phase_ms,
                "counters": self.counters,
                "budgets": self.budgets,
            });
            if let Err(error) = append_jsonl(path, &event) {
                eprintln!("telemetry-json write failed: {error}");
            }
        }
    }
}

fn append_jsonl(path: &Path, event: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_json() -> PathBuf {
        std::env::temp_dir().join(format!(
            "zstdf-telemetry-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn jsonl_records_phases_and_terminal_events() {
        let path = temp_json();
        let options = TelemetryOptions {
            json_path: Some(path.clone()),
        };
        let mut telemetry = RunTelemetry::start("convert", &options);
        telemetry.counter("rows", 3);
        telemetry.budget("memory_mib", 10, 256);
        telemetry.phase("write");
        telemetry.finish_ok();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(text.contains("\"command\":\"convert\""));
        assert!(text.contains("\"phase\":\"start\""));
        assert!(text.contains("\"phase\":\"write\""));
        assert!(text.contains("\"status\":\"ok\""));
        assert!(text.contains("\"rows\":3"));
    }

    #[test]
    fn omitted_json_path_still_emits_stderr_only() {
        let mut telemetry = RunTelemetry::start("view", &TelemetryOptions::default());
        telemetry.finish_cancelled();
    }
}
