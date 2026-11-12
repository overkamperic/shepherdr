//! Restore an isolated board's durable budget binding before any hook, runner or interactive command can launch work.
use crate::err::Res;
use crate::json::*;
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf { crate::store::files::data_dir().join("pipeline-budget.json") }
pub fn environment(file: &Path, current: impl Fn(&str) -> Option<String>) -> Res<Vec<(String, String)>> {
    if !file.exists() { return Ok(vec![]); }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let mut missing = vec![];
    for (key, field) in [("PIPELINE_BUDGET_MANIFEST", "approval"), ("PIPELINE_BUDGET_DB", "database"),
                        ("PIPELINE_BUDGET_CONTROL", "control"), ("CODEX_REQUEST_GATE", "gate"),
                        ("HERDR_PROJECTS_CONFIG_DIR", "config")] {
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
    Ok(missing)
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
