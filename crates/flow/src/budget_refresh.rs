//! Fresh worker contexts after a spending-controller checkpoint, preserving task evidence and counters.
use crate::err::Res;
use crate::json::*;
use crate::runner::{claim, release, Guard};
use crate::store::files::dir_of;
use std::path::PathBuf;

pub fn refresh(id: &str) -> Res<Value> {
    if !crate::budget_runtime::enabled() { crate::fail!("context refresh needs a bounded run"); }
    let db = std::env::var("PIPELINE_BUDGET_DB").map_err(|_| crate::err::Error::new("budget database missing"))?;
    let checkpoint: Value = serde_json::from_str(&std::fs::read_to_string(PathBuf::from(db).with_extension("checkpoint.json"))?)?;
    if checkpoint.s("task") != id || !checkpoint.b("lease") { crate::fail!("checkpoint belongs to another task"); }
    if !["context checkpoint required before a fresh worker", "native context reached threshold"].contains(&checkpoint.s("reason")) {
        crate::fail!("only a context checkpoint can refresh automatically");
    }
    let Some(t) = claim(id, "budget-refresh", |_| {})? else { crate::fail!("native work still owns this task; checkpoint retained"); };
    let mut g = Guard::new(id);
    g.expect(&t, &[]);
    let result = refresh_owned(&mut g, &checkpoint);
    release(id, false);
    if result.as_ref().is_ok_and(|r| !r.b("pending")) && checkpoint.s("role") == "qa" {
        crate::actions::finish_line::retry(id)?;
    }
    result
}

fn refresh_owned(g: &mut Guard, checkpoint: &Value) -> Res<Value> {
    let mut t = g.check()?;
    if !crate::store::marks::pending_events(&t).is_empty() { crate::fail!("native completion is still settling; checkpoint retained"); }
    let qa = checkpoint.s("role") == "qa";
    if qa && t.s("stage") != "failed" { crate::fail!("review must settle before a fresh independent review"); }
    let pane = if qa { t.at("link").at("reviewer").t("pane") } else { t.at("link").t("pane") };
    let api = crate::flow::task::h(&t);
    if t.at("budgetHandoff").s("old_lease") == checkpoint.s("lease") && !pane.is_empty() && api.agent_get(&pane)?.is_some() {
        return Ok(json!({"task": t.at("id"), "fresh_context": false, "pending": true}));
    }
    if !pane.is_empty() {
        if api.pane_get(&pane).is_none() { crate::fail!("native pane status unknown"); }
        if api.agent_get(&pane)?.is_some() && !api.pane_quit(&pane, 6000) { crate::fail!("old native worker did not stop"); }
        if api.agent_get(&pane)?.is_some() { crate::fail!("old native worker is still present"); }
    } else if !qa { crate::fail!("worker pane missing"); }
    let folder = crate::store::repos::cwd_of(&t);
    let head = crate::flow::git::head_of(&folder);
    let words = crate::finish::hand_over::handover_prompt(&t, t.s("agent"), "context checkpoint", "");
    let path = dir_of(t.s("id")).join("budget-handoff.json");
    let handoff = json!({"lease": checkpoint.at("lease"), "task": t.at("id"), "session_id": checkpoint.at("root_session"),
        "worker_stopped": true, "source_scope": {"title": t.at("title"), "description": t.at("description")}, "scope_digest": checkpoint.at("task_digest"), "head": head,
        "evidence": {"branch": t.at("branch"), "review": t.at("review"), "reviewRounds": t.at("reviewRounds"), "worktree": folder},
        "approval_digest": checkpoint.at("budget").at("approval_digest"), "brief": words});
    std::fs::write(&path, serde_json::to_vec_pretty(&handoff)?)?;
    let control = std::env::var("PIPELINE_BUDGET_CONTROL").map_err(|_| crate::err::Error::new("budget controller missing"))?;
    let fresh_lease = crate::budget_runtime::control_call(&control, &["refresh", "--handoff", &path.display().to_string()])?;
    t = g.set(|t| { t.put("budgetHandoff", json!({"lease": fresh_lease, "old_lease": checkpoint.at("lease"), "state": "starting"})); })?;
    if !qa {
        t = g.set(|t| {
            t.put("session", Value::Null); t.put("transcript", Value::Null);
            let h = t.obj_mut("handled"); h.put("msg", Value::Null); h.put("pos", 0);
        })?;
        crate::delegate::start_agent(t.s("id"), g, &words, None)?;
        g.set(|t| t.obj_mut("budgetHandoff").put("state", "launched"))?;
    }
    Ok(json!({"task": t.at("id"), "checkpoint": path.display().to_string(), "fresh_context_requested": true}))
}
