//! Mandatory controller leases for bounded Codex launches, independent of optional notification hooks.
use crate::err::Res;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn enabled() -> bool { std::env::var_os("PIPELINE_BUDGET_MANIFEST").is_some() || crate::budget_binding::path().exists() }
pub fn mandatory_args(kind: &str, lease: &str) -> Res<Vec<String>> {
    if kind != "codex" || lease.is_empty() || lease.len() > 128 || !lease.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        crate::fail!("bounded runs require a supported Codex runtime and controller lease");
    }
    let mut args = vec!["--pipeline-lease".into(), lease.into(), "--no-daemon".into(),
        "--sandbox".into(), "workspace-write".into(), "--ask-for-approval".into(), "never".into()];
    for feature in ["codex_hooks", "apps", "plugins", "code_mode", "code_mode_only"] {
        args.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    args.extend(["-c".into(), "sandbox_workspace_write.network_access=false".into()]);
    Ok(args)
}
pub fn launch(task: &str, role: &str, kind: &str, round: i64, scope: &crate::json::Value) -> Res<Vec<String>> {
    if !enabled() { return Ok(vec![]); }
    if kind != "codex" { crate::fail!("bounded runtime does not support {kind}; no provider fallback is permitted"); }
    let control = std::env::var("PIPELINE_BUDGET_CONTROL").map_err(|_| crate::err::Error::new("bounded controller command missing"))?;
    if !Path::new(&control).is_absolute() { crate::fail!("bounded controller command must be absolute"); }
    let scope_file = crate::store::files::dir_of(task).join(format!("budget-scope-{role}-{round}.json"));
    std::fs::write(&scope_file, serde_json::to_vec(scope)?)?;
    let lease = control_call(&control, &["lease", "--task", task, "--role", role, "--slot", &round.to_string(), "--scope-file", &scope_file.display().to_string()])?;
    let mut args = mandatory_args(kind, &lease)?;
    let output = crate::store::files::dir_of(task).join(if role == "qa" { "review" } else { "bounded-output" });
    std::fs::create_dir_all(&output)?;
    args.extend(["--add-dir".into(), output.display().to_string()]);
    Ok(args)
}
pub fn environment() -> Vec<(String, String)> {
    if !enabled() { return vec![]; }
    ["PIPELINE_BUDGET_MANIFEST", "PIPELINE_BUDGET_DB", "PIPELINE_BUDGET_CONTROL", "CODEX_REQUEST_GATE"]
        .into_iter().filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
        .chain(std::iter::once(("CODEX_REQUEST_GATE_REQUIRED".into(), "1".into()))).collect()
}

pub fn control_call(control: &str, args: &[&str]) -> Res<String> {
    if !Path::new(control).is_absolute() { crate::fail!("bounded controller command must be absolute"); }
    let child = Command::new(control).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let answer = crate::herdr::cli::wait_with_timeout(child, 5000)?;
    if answer.status != 0 { crate::fail!("bounded action refused by spending controller"); }
    Ok(answer.stdout.trim().to_string())
}

pub fn repair(task: &str, reason: &str, turn: i64) -> Res<()> {
    if !enabled() { return Ok(()); }
    if reason == "retry" { crate::fail!("bounded recovery requires reconciled accounting; automatic model nudges are disabled"); }
    let control = std::env::var("PIPELINE_BUDGET_CONTROL").map_err(|_| crate::err::Error::new("budget controller missing"))?;
    let claim = format!("native:{task}:{reason}:{turn}");
    let reply = control_call(&control, &["repair", "--task", task, "--claim", &claim])?;
    if reply != "REPAIR" { crate::fail!("run-wide repair allowance unavailable"); }
    Ok(())
}
