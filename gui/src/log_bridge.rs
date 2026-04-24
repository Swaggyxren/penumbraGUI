/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

//! A `log::Log` adapter that forwards every log record to an `mpsc::Sender`
//! so the GUI can render it in the execution log pane.
//!
//! A minimal "target=level[,...]" parser is used to honour the `RUST_LOG`
//! environment variable without pulling in the private `env_logger::filter`
//! module (which was made pub(crate) in env_logger 0.11).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};

use crate::messages::LogLine;

/// Sink that delivers log records into the GUI's event channel.
pub struct ChannelLogger {
    sender: Mutex<Sender<LogLine>>,
    default: LevelFilter,
    per_target: HashMap<String, LevelFilter>,
}

impl ChannelLogger {
    fn new(sender: Sender<LogLine>, verbose: bool) -> Self {
        let (default, per_target) = parse_filter(&std::env::var("RUST_LOG").ok(), verbose);
        Self { sender: Mutex::new(sender), default, per_target }
    }

    fn level_for(&self, target: &str) -> LevelFilter {
        // Pick the longest matching prefix, matching env_logger's default behaviour.
        let mut best: Option<(usize, LevelFilter)> = None;
        for (key, lvl) in &self.per_target {
            if target == key || target.starts_with(&format!("{key}::")) {
                let len = key.len();
                if best.map(|(l, _)| len > l).unwrap_or(true) {
                    best = Some((len, *lvl));
                }
            }
        }
        best.map(|(_, l)| l).unwrap_or(self.default)
    }
}

impl Log for ChannelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level_for(metadata.target())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = LogLine {
            level: record.level(),
            target: record.target().to_string(),
            message: format!("{}", record.args()),
        };

        if let Ok(tx) = self.sender.lock() {
            let _ = tx.send(line);
        }
    }

    fn flush(&self) {}
}

/// Install a global channel logger that emits into `sender`.
///
/// This can only be called once per process.
pub fn init(sender: Sender<LogLine>, verbose: bool) -> Result<(), SetLoggerError> {
    let logger = ChannelLogger::new(sender, verbose);

    // Compute the maximum filter across all known rules so the log macros do
    // not short-circuit records that a per-target rule would have accepted.
    let max_level = logger
        .per_target
        .values()
        .copied()
        .chain(std::iter::once(logger.default))
        .max()
        .unwrap_or(LevelFilter::Info);

    log::set_max_level(max_level);
    log::set_boxed_logger(Box::new(logger))
}

fn parse_filter(
    raw: &Option<String>,
    verbose: bool,
) -> (LevelFilter, HashMap<String, LevelFilter>) {
    let default_level = if verbose { LevelFilter::Debug } else { LevelFilter::Info };

    let Some(raw) = raw else {
        return (default_level, HashMap::new());
    };

    let mut default = default_level;
    let mut per_target: HashMap<String, LevelFilter> = HashMap::new();

    for spec in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match spec.split_once('=') {
            Some((target, level)) => {
                if let Some(lvl) = parse_level(level) {
                    per_target.insert(target.to_string(), lvl);
                }
            }
            None => {
                if let Some(lvl) = parse_level(spec) {
                    default = lvl;
                }
            }
        }
    }

    (default, per_target)
}

fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}
