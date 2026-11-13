//! Trial authorization through planner fallback, queued messages, and worker recovery.
use crate::{budget_runtime::TrialEnv, common};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use sw::json::{Js, JsMut};

fn bind(w: &common::World) -> (TrialEnv, PathBuf, PathBuf) {
    let controller = w.base.join("controller");
    let log = w.base.join("leases");
    std::fs::write(&controller, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = repair ]; then echo REPAIR; else echo 'LEASE lease-1'; fi\n", log.display())).unwrap();
    std::fs::set_permissions(&controller, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = sw::budget_binding::path();
    let binding = json!({"approval":"/approval","database":"/db","control":controller,"gate":controller,"config":sw::store::files::config_dir(),"reports":w.base.join("reports"),"trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&binding).unwrap()).unwrap();
    (TrialEnv::install(&file), controller, log)
}

#[test]
fn real_programs_trial_missing_planner_never_falls_back() {
    let w = common::world_with(common::Opts { repo:true, settings:json!({"worker.agent":"codex", "plan.agent":"claude", "plan.approve":"you"}), ..Default::default() });
    unsafe { std::env::set_var("HERDR_PROJECTS_SANDBOX", "bwrap"); }
    let id = w.add("Plan task");
    let versions = sw::store::files::data_dir().join("versions.json");
    let mut data: serde_json::Value = serde_json::from_slice(&std::fs::read(&versions).unwrap()).unwrap();
    data["claude"]["found"] = json!(false);
    std::fs::write(&versions, serde_json::to_vec(&data).unwrap()).unwrap();
    let (_env, _, log) = bind(&w);
    common::delegate(&id);
    let t = common::settle(&id);
    assert!(t.at("failure").to_string().contains("fallback refused"), "{}", serde_json::Value::Object(t));
    assert!(!log.exists());
}

#[test]
fn real_programs_trial_denial_keeps_queued_text_and_refuses_restart() {
    let w = common::world_with(common::Opts { repo:true, settings:json!({"worker.agent":"codex"}), ..Default::default() });
    w.script(json!({"worker":[{"status":"working"}]}));
    let id = w.add("Guard recovery");
    let (_env, controller, log) = bind(&w);
    common::delegate(&id);
    let t = common::settle(&id);
    let pane = t.at("link").t("pane");
    assert!(!pane.is_empty(), "{}", serde_json::Value::Object(t.clone()));
    sw::store::inbox::queue_text(&id, "guarded queued message", sw::json::Obj::new()).unwrap();
    std::fs::write(&controller, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 1\n", log.display())).unwrap();
    common::set_status(&w, &pane, "idle");
    assert_eq!(sw::store::inbox::pending_text(&id).len(), 1);
    assert!(!common::prompts(&w, &pane).iter().any(|s| s.contains("guarded queued message")));
    w.patch(|state| { for p in state["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"] = serde_json::Value::Null; } } });
    sw::delegate::jobs::restart_job(&id);
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("--action continue"), "{calls}");
    assert!(calls.matches("--action launch").count() >= 2, "{calls}");
    assert!(sw::store::tasks::get(&id).unwrap().at("failure").to_string().contains("controller"));
    let sessions = w.base.join("codex/sessions/2026");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("rollout-2026-resume-id.jsonl"), "{}\n").unwrap();
    sw::store::tasks::update(&id, |t| { t.put("session", "resume-id"); Ok(sw::store::tasks::Change::Write) }).unwrap();
    let (_, has_agent, _) = sw::delegate::jobs::reopen(&id, true).unwrap();
    assert!(!has_agent);
    let calls = std::fs::read_to_string(log).unwrap();
    assert!(calls.matches("--action launch").count() >= 3, "{calls}");
}

#[test]
fn real_programs_trial_repair_needs_both_allowance_and_continuation_lease() {
    let w = common::world();
    let id = w.add("Repair task");
    let (_env, controller, log) = bind(&w);
    sw::store::tasks::update(&id, |t| { t.put("agent", "codex"); t.put("trialModel", "gpt-6.1-sol"); t.put("armedAt", 123); Ok(sw::store::tasks::Change::Write) }).unwrap();
    let _ = sw::flow::send::send_own(&id, "Fix it", "review", false, sw::json::Obj::new(), 0);
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains(&format!("repair --task {id} --claim native:{id}:review:123")), "{calls}");
    assert!(calls.contains("--role repair") && calls.contains("--action continue"), "{calls}");
    std::fs::write(&controller, "#!/bin/sh\nexit 1\n").unwrap();
    let before = std::fs::read_to_string(&log).unwrap();
    assert!(sw::flow::send::send_own(&id, "Fix it", "review", false, sw::json::Obj::new(), 0).is_err());
    assert_eq!(std::fs::read_to_string(log).unwrap(), before);
    sw::store::tasks::update(&id, |t| { t.put("stage", "failed"); t.put("failure", json!({"at":1})); t.put("waiting", json!({"who":"agent", "for":1, "until":0, "tries":1})); Ok(sw::store::tasks::Change::Write) }).unwrap();
    assert!(!sw::flow::send::resume_cut(&id));
    assert_eq!(sw::store::tasks::get(&id).unwrap().at("waiting").s("who"), "agent");
}
