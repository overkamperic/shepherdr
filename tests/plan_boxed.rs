//! A plan writer runs in a read-only mount, and so does its own turn-end report: the runner that report would start
//! stays in the mount and cannot make the worktree writable for the build (F14). It is started from outside the mount
//! instead, by the housekeeping. And a worktree that cannot be made writable holds the build until a retry.
use crate::common;
use common::*;
use serde_json::json;
use std::path::Path;
use sw::json::*;
use sw::plan::lock::is_locked;

const PLAN: &str = "# Plan\n\n1. Add f.txt with one line.\n2. Run the check.\n";
fn plan_world() -> World {
    let w = world_with(Opts { repo: true, settings: json!({ "plan.approve": "reviewer", "worker.agent": "claude", "review.when": "never" }), ..Default::default() });
    unsafe { std::env::set_var("HERDR_PROJECTS_SANDBOX", "bwrap") };
    w.patch(|s| s["pids"] = json!(true));
    w.script(json!({ "worker": [{ "plan": PLAN, "note": "Written.", "hook": true }, { "status": "working" }],
        "reviewer": [{ "verdict": { "meets": true, "findings": [], "text": "A good plan." } }] }));
    w
}
/// The board's next detached job, run now.
fn run_next() -> String {
    let Some((job, id, extra)) = sw::runner::JOBS.with(|j| { let mut j = j.borrow_mut(); (!j.is_empty()).then(|| j.remove(0)) }) else { return String::new() };
    sw::jobs::run_job(&job, &id, &extra);
    flush();
    job
}
fn queued() -> Vec<String> { sw::runner::JOBS.with(|j| j.borrow().iter().map(|(job, _, _)| job.clone()).collect()) }
/// The planner's own turn-end report, run as its hook runs: in its read-only mount, well after the start's runner ended.
fn report_in_the_mount(id: &str) {
    let mut dir = sw::store::files::data_dir().join("tasks"); for part in id.split('/') { dir.push(part); }
    let token = serde_json::from_str::<serde_json::Value>(&read(&dir.join("tokens.json"))).unwrap().t("agent");
    unsafe { std::env::set_var("HERDR_PROJECTS_BOXED", "1") };
    sw::turn::turn_ended(id, "agent", &token, 0, &["{}".to_string()]);
    unsafe { std::env::remove_var("HERDR_PROJECTS_BOXED") };
}

#[test]
fn a_plan_whose_turn_ends_in_the_read_only_mount_is_built_from_outside_it_in_a_writable_worktree() {
    let w = plan_world();
    // Busy as herdr sees it: only its own report says the turn ended, as it said first in trial 2.
    w.script(json!({ "worker": [{ "plan": PLAN, "note": "Written.", "status": "working" }, { "status": "working" }],
        "reviewer": [{ "verdict": { "meets": true, "findings": [], "text": "A good plan." } }] }));
    let id = w.add("Work");
    delegate(&id);
    assert_eq!(run_next(), "delegate");
    assert!(queued().is_empty(), "{:?}", queued());
    let t = task(&id);
    assert_eq!(agent_of(&w, t.at("link").s("pane")).at("boxed"), &json!(true));
    assert!(is_locked(&t) && !t.b("runner"));
    report_in_the_mount(&id);
    let t = task(&id);
    // The planner's own report ran in the mount: it starts no runner there, and leaves it owed.
    assert!(!queued().contains(&"finish".to_string()), "a runner started in the mount: {:?}", queued());
    assert!(t.b("finishOwed"));
    // The housekeeping, outside the mount, starts it: the reviewer passes the plan and the build starts.
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    assert_eq!(queued(), vec!["finish".to_string()]);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working", "{}", t.at("failure"));
    assert_eq!(t.at("plan").s("approvedBy"), "reviewer");
    assert!(t.at("plan").b("built") && !t.b("finishOwed"));
    assert!(!is_locked(&t), "the build starts in a writable worktree");
    std::fs::write(Path::new(t.s("worktree")).join("f.txt"), "built\n").unwrap();
}

#[test]
fn a_worktree_that_cannot_be_made_writable_holds_the_build_and_a_retry_starts_it() {
    let w = plan_world();
    let id = w.add("Work");
    delegate(&id);
    assert_eq!(run_next(), "delegate");
    let t = task(&id);
    // A folder in the worktree that its owner cannot read: it cannot be made writable again.
    let tree = Path::new(t.s("worktree")).to_path_buf();
    let mode = |p: &Path, m: u32| { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap() };
    mode(&tree, 0o755); std::fs::create_dir(tree.join("shut")).unwrap(); mode(&tree.join("shut"), 0o000); mode(&tree, 0o555);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    assert_eq!(t.at("failure").s("title"), "The worktree could not be made writable");
    assert_eq!(t.at("plan").s("approvedBy"), "reviewer");
    assert!(!t.at("plan").b("built"), "no build in a worktree that is still read-only");
    assert_eq!(t.at("plan").at("locked"), &json!(true), "still locked, so the clean-up unlocks it");
    mode(&tree.join("shut"), 0o755);
    sw::actions::finish_line::retry(&id).unwrap();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working", "{}", t.at("failure"));
    assert!(t.at("plan").b("built") && !is_locked(&t));
}

#[test]
fn an_owed_finish_survives_a_lost_runner_start_while_the_agent_still_looks_busy() {
    let w = plan_world();
    w.script(json!({ "worker": [{ "plan": PLAN, "note": "Written.", "status": "working" }, { "status": "working" }],
        "reviewer": [{ "verdict": { "meets": true, "findings": [], "text": "A good plan." } }] }));
    let id = w.add("Work");
    delegate(&id);
    assert_eq!(run_next(), "delegate");
    report_in_the_mount(&id);
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    assert_eq!(queued(), vec!["finish".to_string()]);
    // A spawn that failed, or a process that died before claiming the task, leaves no runner marker.
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
    sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes);
    assert_eq!(queued(), vec!["finish".to_string()], "the owed report must remain recoverable");
    let t = settle(&id);
    assert!(t.at("plan").b("built") && !t.b("finishOwed"));
    assert!(!is_locked(&t));
}
