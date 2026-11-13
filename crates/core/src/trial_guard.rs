//! Trial run binding and fail-closed controller authorization.
use crate::err::Res;
use crate::json::*;
use std::path::Path;

/// Ask the controller for permission and return the explicit model and execution lease.
#[allow(clippy::too_many_arguments)]
pub fn authorize(binding: &Path, scope: &Path, task: &str, role: &str, slot: i64, action: &str, kind: &str, model: &str) -> Res<(String, String)> {
    let saved: Value = serde_json::from_slice(&std::fs::read(binding)?)?;
    let trial = crate::budget_binding::durable_trial(binding, &saved)?.ok_or("trial binding missing")?;
    use std::os::unix::fs::PermissionsExt;
    let gate = Path::new(saved.s("gate"));
    if !gate.is_absolute() || !std::fs::metadata(gate).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0) {
        crate::fail!("trial request gate missing or not executable");
    }
    if kind != "codex" || (!model.is_empty() && model != trial.s("model")) { crate::fail!("trial model or program differs from the pinned model"); }
    if !["planning", "implementation", "qa", "repair"].contains(&role) || !["launch", "continue"].contains(&action) || slot < 0 {
        crate::fail!("invalid trial role or action");
    }
    let control = saved.s("control");
    if !Path::new(control).is_absolute() { crate::fail!("trial controller missing"); }
    let answer = crate::budget_runtime::control_call(control, &["trial-lease", "--schema", "1", "--run", trial.s("run_id"),
        "--token-ceiling", &trial.at("token_ceiling").to_string(), "--model", trial.s("model"), "--task", task,
        "--role", role, "--slot", &slot.to_string(), "--action", action, "--scope-file", &scope.display().to_string()])?;
    let lease = answer.strip_prefix("LEASE ").ok_or("invalid trial controller response")?;
    if lease.is_empty() || lease.len() > 128 || !lease.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        crate::fail!("invalid trial controller lease");
    }
    Ok((trial.t("model"), lease.into()))
}
