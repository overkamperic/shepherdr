//! [real programs] Completed-task finalization reports verified native ownership as typed JSON while preserving human errors and hard active-work failures.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::actions::{archive, filing};
use sw::json::*;
use sw::store::tasks::{update, Change};

fn put(id: &str, stage: &str, job: &str) {
    update(id, |t| {
        t.put("stage", stage);
        t.obj_mut("link").put("pane", "owned-worker");
        let mut m = sw::store::files::marker(); m.put("job", job); t.put("runner", m);
        Ok(Change::Write)
    }).unwrap();
}
fn run(w: &World, args: &[&str]) -> (i32, String, String) {
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_herdr-projects")).args(args).current_dir(&w.root)
        .env("HERDR_SOCKET_PATH", "/nonexistent").env("HERDR_BIN_PATH", "/bin/false").output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}
fn report(id: &str) -> sw::err::Res<Obj> { filing::report(id, filing::Report { state: "done".into(), ..Default::default() }) }

#[test]
fn real_programs_completed_ownership_is_typed_for_report_and_archive_without_mutation() {
    let w = world(); let id = w.add("Completed");
    for stage in ["approval", "done"] { for job in ["finish", "sync", "cleanup", "future-native-job"] {
        put(&id, stage, job);
        let before = task(&id);
        for args in [vec!["report", &id, "--state", "done", "--json"], vec!["archive", &id, "--json"]] {
            let (code, stdout, stderr) = run(&w, &args);
            assert_eq!(code, 1, "{args:?}: {stdout} {stderr}"); assert!(stdout.is_empty());
            let error: Value = serde_json::from_str(&stderr).expect("--json must give a machine-readable ownership error");
            assert_eq!(error.at("error").s("code"), "ownership_busy", "{error}");
            assert!(!error.at("error").s("message").is_empty());
            assert_eq!(task(&id), before, "refusal must not change stage, title, runner or cleanup state");
        }
    } }
}

#[test]
fn real_programs_ownership_keeps_existing_human_messages_and_unrelated_json_errors_are_hard() {
    let w = world(); let id = w.add("Owned"); put(&id, "approval", "sync");
    let (code, _, error) = run(&w, &["report", &id, "--state", "done"]);
    assert_eq!(code, 1); assert_eq!(error, "PRJ-1 is in the middle of a workflow step. Stop it first.\n");
    let (_, _, error) = run(&w, &["archive", &id]);
    assert_eq!(error, "PRJ-1 is still active. Stop its agent first; an automatic merge must finish first.\n");
    put(&id, "done", "cleanup");
    let (_, _, error) = run(&w, &["archive", &id]);
    assert_eq!(error, "PRJ-1 is busy cleaning up after its merge. Try again in a moment.\n");
    for args in [vec!["report", &id, "--state", "nonsense", "--json"], vec!["archive", "missing/99", "--json"]] {
        let (code, _, stderr) = run(&w, &args); assert_eq!(code, 1);
        let error: Value = serde_json::from_str(&stderr).expect("JSON errors must remain parseable");
        assert_ne!(error.at("error").s("code"), "ownership_busy");
    }
}

#[test]
fn unfinished_work_and_generic_lock_errors_are_not_completed_ownership() {
    let w = world(); let id = w.add("Active");
    for stage in ["working", "checking", "reviewing", "merging", "queued", "attempts", "asks", "failed"] {
        put(&id, stage, "sync");
        assert_eq!(archive::archive(&id).unwrap_err().kind, sw::err::Kind::Plain);
        assert_eq!(report(&id).unwrap_err().kind, sw::err::Kind::Plain);
    }
    let busy = sw::err::Error::busy("task lock busy");
    let error: Value = serde_json::from_str(&sw::cli::errors::say(&busy, true)).unwrap();
    assert_eq!(error.at("error").s("code"), "busy", "generic lock contention must never become ownership_busy");
    put(&id, "done", "sync");
    update(&id, |t| { t.put("runner", Value::Null); t.put("mergeIntent", json!({"kind": "merge"})); Ok(Change::Write) }).unwrap();
    assert_eq!(archive::archive(&id).unwrap_err().kind, sw::err::Kind::Plain, "unfinished merge intent remains hard");
    update(&id, |t| { t.put("archivedAt", sw::clock::now()); Ok(Change::Write) }).unwrap();
    put(&id, "done", "sync");
    assert!(archive::archive(&id).unwrap().b("archivedAt"), "an already archived task remains idempotent even with an old live marker");
    assert_eq!(report(&id).unwrap_err().kind, sw::err::Kind::Plain, "reporting an archived task remains a hard error");
}

#[test]
fn released_dead_or_reused_process_markers_allow_finalization_and_successful_json_stays_unchanged() {
    let w = world(); let id = w.add("Release");
    for marker in [Value::Null, json!({"pid": 4194381, "start": "0", "job": "sync"}), json!({"pid": std::process::id(), "start": "wrong-start", "job": "sync"})] {
        put(&id, "approval", "sync");
        update(&id, |t| { t.put("runner", marker.clone()); Ok(Change::Write) }).unwrap();
        assert_eq!(report(&id).unwrap().s("stage"), "done");
        // report-done schedules cleanup, but a queued job is not a live owner.
        assert!(archive::archive(&id).unwrap().b("archivedAt"));
        sw::actions::archive::restore(&id).unwrap();
    }
    update(&id, |t| { t.put("link", Value::Null); Ok(Change::Write) }).unwrap();
    let (code, stdout, stderr) = run(&w, &["report", &id, "--state", "done", "--json"]);
    assert_eq!(code, 0, "{stderr}"); let success: Value = serde_json::from_str(&stdout).unwrap();
    assert!(!success.has("error")); assert!(success.b("id"));
}
