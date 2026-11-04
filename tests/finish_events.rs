//! A saved worker-end report whose detached finish runner was lost: pane reconciliation must recover the report
//! even when herdr supplies no completion counter, without finishing a side chat or a human wait.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;

fn ended_worker() -> (World, String) {
    let w = world_with(Opts { repo: true, settings: json!({ "worker.agent": "claude" }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "answer.txt": "done
" }, "commit": "Work",
        "ends": "claude-finished-made", "status": "working" }] }));
    let id = w.add("Recover saved completion");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "working");
    let t = task(&id);
    let mut dir = sw::store::files::data_dir().join("tasks");
    for part in id.split('/') { dir.push(part); }
    let tokens: Value = serde_json::from_str(&read(&dir.join("tokens.json"))).unwrap();
    sw::turn::turn_ended(&id, "agent", tokens.s("agent"), 0, &["{}".to_string()]);
    assert!(!sw::store::marks::pending_events(&task(&id)).is_empty());
    // The hook saved its event, but the detached runner never claimed it (a failed start or a crash).
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
    assert!(!task(&id).b("runner"));
    let pane = t.at("link").t("pane");
    w.patch(|s| {
        let seq = s.i("seq") + 1; s["seq"] = json!(seq);
        for p in s["panes"].as_array_mut().unwrap() {
            if p.s("pane_id") == pane {
                p["agent"]["status"] = json!("done");
                p["agent"]["completion"] = Value::Null;
                p["agent"]["seq"] = json!(seq);
            }
        }
    });
    (w, id)
}

#[test]
fn an_unhandled_worker_end_is_recovered_without_herdr_counting_the_completion() {
    let (w, id) = ended_worker();
    let pane = task(&id).at("link").t("pane");
    let before = prompts(&w, &pane).len();
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
    assert!(t.at("review").b("pass"));
    assert!(!t.b("turnOpen"));
    assert!(sw::store::marks::pending_events(&t).is_empty());
    assert_eq!(prompts(&w, &pane).len(), before, "completed work must not be prompted again");
    settle(&id);
    let reviews = calls(&w).iter().filter(|c| regex::Regex::new(r"^agent start \S+-r ").unwrap().is_match(c)).count();
    assert_eq!(reviews, 1, "reconciliation must not dispatch a second review");
}

#[test]
fn reviewer_and_other_side_session_events_do_not_finish_the_worker() {
    for (role, side, session) in [("reviewer", false, ""), ("agent", true, "another-session")] {
        let (_w, id) = ended_worker();
        sw::store::tasks::patch(&id, |t| {
            for e in t.get_mut("events").unwrap().as_array_mut().unwrap() { e["role"] = json!(role); e["side"] = json!(side); e["session"] = json!(session); }
        }).unwrap();
        let t = settle(&id);
        assert_eq!(t.s("stage"), "working");
        assert!(!t.b("review"));
    }
}

#[test]
fn a_saved_end_does_not_answer_a_human_wait() {
    let (_w, id) = ended_worker();
    sw::store::tasks::patch(&id, |t| { t.put("stage", "asks"); t.put("msg", "Need a decision"); }).unwrap();
    let t = settle(&id);
    assert_eq!((t.s("stage"), t.s("msg")), ("asks", "Need a decision"));
    assert!(!t.b("review"));
}
