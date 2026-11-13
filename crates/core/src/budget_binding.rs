//! Restore an isolated board's durable budget binding before any hook, runner or interactive command can launch work.
use crate::err::Res;
use crate::json::*;
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf { crate::store::files::data_dir().join("pipeline-budget.json") }
pub fn environment(file: &Path, current: impl Fn(&str) -> Option<String>) -> Res<Vec<(String, String)>> {
    if !file.exists() {
        if current("PIPELINE_TRIAL_REQUIRED").is_some() || file.with_file_name("pipeline-trial.json").exists() { crate::fail!("trial binding missing"); }
        return Ok(vec![]);
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let trial = durable_trial(file, &saved)?;
    let mut missing = vec![];
    for (key, field) in [("PIPELINE_BUDGET_MANIFEST", "approval"), ("PIPELINE_BUDGET_DB", "database"),
                        ("PIPELINE_BUDGET_CONTROL", "control"), ("CODEX_REQUEST_GATE", "gate"),
                        ("HERDR_PROJECTS_CONFIG_DIR", "config"), ("HERDR_PROJECTS_REPORTS_DIR", "reports")] {
        let value = saved.s(field);
        if value.is_empty() || !Path::new(value).is_absolute() { crate::fail!("durable pipeline binding is incomplete"); }
        match current(key) {
            Some(old) if old == value => {},
            Some(_) => crate::fail!("{key} conflicts with the durable pipeline binding"),
            None => missing.push((key.into(), value.into())),
        }
    }
    match current("CODEX_REQUEST_GATE_REQUIRED") {
        Some(v) if v == "1" => {},
        Some(_) => crate::fail!("mandatory request guard cannot be disabled"),
        None => missing.push(("CODEX_REQUEST_GATE_REQUIRED".into(), "1".into())),
    }
    if trial.is_some() {
        match current("PIPELINE_TRIAL_REQUIRED") {
            Some(v) if v == "1" => {},
            Some(_) => crate::fail!("trial binding cannot be disabled"),
            None => missing.push(("PIPELINE_TRIAL_REQUIRED".into(), "1".into())),
        }
    } else if current("PIPELINE_TRIAL_REQUIRED").is_some() { crate::fail!("trial binding missing"); }
    Ok(missing)
}

/// Validate the immutable trial tuple; the controller independently checks its authoritative copy.
pub fn trial(saved: &Value) -> Res<Option<Value>> {
    let v = saved.at("trial");
    if v.is_null() { return Ok(None); }
    let id = v.s("run_id");
    let model = v.s("model");
    if v.at("schema").as_u64() != Some(1) || id.is_empty() || id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || v.at("token_ceiling").as_u64().is_none_or(|n| n == 0)
        || model.is_empty() || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
        crate::fail!("invalid trial binding");
    }
    Ok(Some(v.clone()))
}

/// Keep an independent durable copy, so removing or changing the binding fails closed on later starts.
pub fn durable_trial(file: &Path, saved: &Value) -> Res<Option<Value>> {
    use std::io::Write;
    let trial = trial(saved)?;
    let marker = file.with_file_name("pipeline-trial.json");
    if trial.is_some() {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&marker) {
            Ok(mut f) => { f.write_all(&serde_json::to_vec(saved)?)?; f.sync_all()?; },
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(e) => return Err(e.into()),
        }
    }
    if marker.exists() {
        let old: Value = serde_json::from_slice(&std::fs::read(marker)?)?;
        if trial.is_none() || &old != saved { crate::fail!("trial binding changed or missing"); }
    }
    Ok(trial)
}

pub fn bootstrap() -> Res<()> {
    let vars = environment(&path(), |key| std::env::var(key).ok())?;
    if vars.is_empty() { return Ok(()); }
    // Re-exec this process; never mutate process-global environment in a live multithreaded board.
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(std::env::current_exe()?)
        .args(std::env::args_os().skip(1)).envs(vars).exec();
    Err(error.into())
}
