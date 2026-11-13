//! Bounded launches keep their mandatory lease even when optional notification hooks are absent.
use sw::budget_runtime::mandatory_args;
use crate::common;

pub(super) struct TrialEnv(Vec<(String, Option<std::ffi::OsString>)>);
impl TrialEnv {
    pub(super) fn install(file: &std::path::Path) -> Self {
        let vars = sw::budget_binding::environment(file, |_| None).unwrap();
        let old = vars.iter().map(|(k, _)| (k.clone(), std::env::var_os(k))).collect();
        for (k, v) in vars { unsafe { std::env::set_var(k, v); } }
        Self(old)
    }
}
impl Drop for TrialEnv {
    fn drop(&mut self) { for (k, v) in &self.0 { unsafe { if let Some(v) = v { std::env::set_var(k, v); } else { std::env::remove_var(k); } } } }
}

#[test]
fn budget_runtime_unsupported_program_is_refused() {
    assert!(mandatory_args("claude", "lease-1").is_err());
    assert!(mandatory_args("codex", "").is_err());
}
#[test]
fn budget_runtime_lease_is_independent_of_hooks() {
    let args = mandatory_args("codex", "lease-1").unwrap();
    assert!(args.windows(2).any(|a| a == ["--pipeline-lease", "lease-1"]));
    assert!(args.windows(2).any(|a| a == ["--sandbox", "workspace-write"]));
    assert!(args.windows(2).any(|a| a == ["--ask-for-approval", "never"]));
    assert!(!args.iter().any(|a| a.contains("notify")));
}

#[test]
fn budget_runtime_background_entries_restore_or_refuse_binding() {
    let world = common::world();
    let file = world.base.join("pipeline-budget.json");
    std::fs::write(&file, r#"{"approval":"/approved/run.json","database":"/approved/budget.db","control":"/release/control","gate":"/release/gate","config":"/isolated/settings","reports":"/isolated/reports"}"#).unwrap();
    let vars = sw::budget_binding::environment(&file, |_| None).unwrap();
    assert_eq!(vars.len(), 7);
    assert!(vars.iter().any(|v| v == &("CODEX_REQUEST_GATE_REQUIRED".into(), "1".into())));
    assert!(sw::budget_binding::environment(&file, |_| Some("wrong".into())).is_err());
    std::fs::write(&file, "{}").unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_err());
}

#[test]
fn real_programs_budget_runtime_restores_hook_environment() {
    let world = common::world();
    let dir = world.base.join("bound-board");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pipeline-budget.json"), r#"{"approval":"/approved/run.json","database":"/approved/budget.db","control":"/release/control","gate":"/release/gate","config":"/isolated/settings","reports":"/isolated/reports"}"#).unwrap();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_herdr-projects"));
    cmd.arg("capabilities").env("HERDR_PROJECTS_DATA_DIR", &dir);
    for key in ["PIPELINE_BUDGET_MANIFEST", "PIPELINE_BUDGET_DB", "PIPELINE_BUDGET_CONTROL", "CODEX_REQUEST_GATE", "CODEX_REQUEST_GATE_REQUIRED", "HERDR_PROJECTS_CONFIG_DIR", "HERDR_PROJECTS_REPORTS_DIR"] { cmd.env_remove(key); }
    let result = cmd.output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["budget_binding_loaded"], true);
    let refused = cmd.env("PIPELINE_BUDGET_DB", "/wrong/database").output().unwrap();
    assert!(!refused.status.success());
}

#[test]
fn budget_runtime_rejects_invalid_trial_binding() {
    let world = common::world();
    let file = world.base.join("pipeline-budget.json");
    for trial in [serde_json::json!({}), serde_json::json!({"schema":1,"run_id":"run-1","token_ceiling":0,"model":"gpt-6.1-sol"}), serde_json::json!({"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":""})] {
        let binding = serde_json::json!({"approval":"/approval","database":"/db","control":"/control","gate":"/gate","config":"/config","reports":"/reports", "trial":trial});
        std::fs::write(&file, serde_json::to_vec(&binding).unwrap()).unwrap();
        assert!(sw::budget_binding::environment(&file, |_| None).is_err());
    }
}

#[test]
fn real_programs_budget_runtime_trial_controller_pins_and_refuses() {
    use std::os::unix::fs::PermissionsExt;
    let w = common::world();
    let control = w.base.join("controller");
    std::fs::write(&control, "#!/bin/sh\n[ \"$1\" = trial-lease ] || exit 1\nprintf 'LEASE lease-1\\n'\n").unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let binding = w.base.join("binding.json");
    let scope = w.base.join("scope.json");
    std::fs::write(&scope, "{}").unwrap();
    let record = serde_json::json!({"control": control, "gate": control, "trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&binding, serde_json::to_vec(&record).unwrap()).unwrap();
    let auth = |kind, model| sw::trial_guard::authorize(&binding, &scope, "test/1", "planning", 0, "launch", kind, model);
    assert_eq!(auth("codex", "").unwrap(), ("gpt-6.1-sol".into(), "lease-1".into()));
    for role in ["planning", "implementation", "qa", "repair"] {
        for action in ["launch", "continue"] {
            let (model, lease) = sw::trial_guard::authorize(&binding, &scope, "test/1", role, 1, action, "codex", "gpt-6.1-sol").unwrap();
            assert_eq!((model.as_str(), lease.as_str()), ("gpt-6.1-sol", "lease-1"));
        }
    }
    assert!(sw::trial_guard::authorize(&binding, &scope, "test/1", "other", 0, "launch", "codex", "").is_err());
    assert!(auth("codex", "other-model").is_err());
    assert!(auth("claude", "").is_err());
    std::fs::write(&control, "#!/bin/sh\necho DENY exhausted\nexit 1\n").unwrap();
    assert!(auth("codex", "").is_err());
}

#[test]
fn budget_runtime_trial_binding_cannot_be_changed_or_removed() {
    let w = common::world();
    let file = w.base.join("pipeline-budget.json");
    let mut saved = serde_json::json!({"approval":"/approval","database":"/db","control":"/control","gate":"/gate","config":"/config","reports":"/reports", "trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_ok());
    let mut changed_control = saved.clone();
    changed_control["control"] = serde_json::json!("/other-controller");
    std::fs::write(&file, serde_json::to_vec(&changed_control).unwrap()).unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_err());
    saved["trial"]["token_ceiling"] = serde_json::json!(2000000);
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_err());
    saved.as_object_mut().unwrap().remove("trial");
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_err());
    std::fs::remove_file(&file).unwrap();
    assert!(sw::budget_binding::environment(&file, |_| None).is_err());
}

#[test]
fn real_programs_budget_runtime_board_launch_uses_trial_protocol() {
    use std::os::unix::fs::PermissionsExt;
    let w = common::world();
    let control = w.base.join("controller");
    std::fs::write(&control, "#!/bin/sh\n[ \"$1\" = trial-lease ] || exit 1\nprintf 'LEASE pinned-lease\\n'\n").unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = sw::budget_binding::path();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let saved = serde_json::json!({"approval":"/approval","database":"/db","control":control,"gate":control,"config":w.base.join("config"),"reports":w.base.join("reports"), "trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    unsafe { std::env::set_var("PIPELINE_BUDGET_CONTROL", control.as_os_str()); }
    let task = serde_json::json!({"id":"test/1", "title":"Test", "description":"Scope"}).as_object().unwrap().clone();
    std::fs::create_dir_all(sw::store::files::dir_of("test/1")).unwrap();
    assert!(sw::delegate::launch_extras(&task, "agent", "codex", 0, "", None).is_err(), "missing request-gate environment must stop a trial launch");
    let binding_env = TrialEnv::install(&file);
    let mut fallback = task.clone();
    fallback.insert("on".into(), serde_json::json!("codex"));
    assert!(sw::delegate::launch_extras(&fallback, "agent", "codex", 0, "", None).is_err());
    assert!(sw::delegate::launch_extras_model(&task, "agent", "codex", "other-model", 0, "", None).is_err());
    let result = sw::delegate::launch_extras(&task, "agent", "codex", 0, "", None);
    drop(binding_env);
    unsafe { std::env::remove_var("PIPELINE_BUDGET_CONTROL"); }
    let extras = result.unwrap();
    assert_eq!(extras.model, "gpt-6.1-sol");
    assert!(extras.mandatory.windows(2).any(|a| a == ["--pipeline-lease", "pinned-lease"]));
}

#[test]
fn real_programs_budget_runtime_continuation_checks_controller_before_send() {
    use std::os::unix::fs::PermissionsExt;
    let w = common::world();
    let control = w.base.join("controller");
    let called = w.base.join("called");
    std::fs::write(&control, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nexit 1\n", called.display())).unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = sw::budget_binding::path();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let saved = serde_json::json!({"approval":"/approval","database":"/db","control":control,"gate":control,"config":w.base.join("config"),"reports":w.base.join("reports"), "trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    std::fs::create_dir_all(sw::store::files::dir_of("test/1")).unwrap();
    let _binding_env = TrialEnv::install(&file);
    let task = serde_json::json!({"id":"test/1", "agent":"codex", "trialModel":"gpt-6.1-sol", "title":"Test", "link":{"pane":"missing"}}).as_object().unwrap().clone();
    assert!(sw::flow::send::deliver(&task, "continue", None, false).is_err());
    let invocation = std::fs::read_to_string(called).expect("controller was not consulted");
    assert!(invocation.contains("--action continue"));
}

#[test]
fn real_programs_budget_runtime_refuses_a_missing_request_gate() {
    use std::os::unix::fs::PermissionsExt;
    let w = common::world();
    let control = w.base.join("controller");
    std::fs::write(&control, "#!/bin/sh\nprintf 'LEASE lease-1\\n'\n").unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let binding = w.base.join("binding.json");
    let scope = w.base.join("scope.json");
    std::fs::write(&scope, "{}").unwrap();
    let record = serde_json::json!({"control":control,"gate":w.base.join("missing-gate"),"trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&binding, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(sw::trial_guard::authorize(&binding, &scope, "test/1", "qa", 1, "launch", "codex", "").is_err());
}

#[test]
fn real_programs_budget_runtime_rejects_automatic_retry_on_trial() {
    use std::os::unix::fs::PermissionsExt;
    use sw::json::JsMut;
    let w = common::world();
    let id = w.add("Retry trial");
    let control = w.base.join("controller");
    std::fs::write(&control, "#!/bin/sh\nprintf 'LEASE lease-1\\n'\n").unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = sw::budget_binding::path();
    let saved = serde_json::json!({"approval":"/approval","database":"/db","control":control,"gate":control,"config":w.base.join("config"),"reports":w.base.join("reports"), "trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let _binding_env = TrialEnv::install(&file);
    sw::store::tasks::update(&id, |t| { t.put("agent", "codex"); t.put("trialModel", "gpt-6.1-sol"); Ok(sw::store::tasks::Change::Write) }).unwrap();
    let err = sw::flow::send::send_own(&id, "Continue", "retry", false, sw::json::Obj::new(), 1).unwrap_err();
    assert!(err.msg.contains("automatic model nudges are disabled"), "{}", err.msg);
}

#[test]
fn real_programs_budget_runtime_trial_cannot_wake_an_existing_coordinator() {
    use sw::json::Js;
    let w = common::world_with(common::Opts { repo: true, settings: serde_json::json!({"coordinator.agent":"claude"}), ..Default::default() });
    w.script(serde_json::json!({"coordinator":[{"status":"working"}]}));
    sw::coordinator::requests::send(&w.root, "first", false, "", &w.socket).unwrap();
    sw::coordinator::pump::pump(&w.socket);
    let before = w.state().to_string();
    assert!(before.contains("first"));
    let file = sw::budget_binding::path();
    let saved = serde_json::json!({"approval":"/approval","database":"/db","control":"/control","gate":"/gate","config":w.base.join("config"),"reports":w.base.join("reports"),"trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let _binding_env = TrialEnv::install(&file);
    sw::coordinator::requests::send(&w.root, "second trial prompt", false, "", &w.socket).unwrap();
    sw::coordinator::pump::pump(&w.socket);
    assert!(!w.state().to_string().contains("second trial prompt"));
    assert!(sw::coordinator::state::get(&w.socket).unwrap().s("error").contains("trial"));
}

#[test]
fn real_programs_budget_runtime_worker_and_reviewer_use_pinned_trial_leases() {
    use std::os::unix::fs::PermissionsExt;
    use sw::json::{Js, text};
    let w = common::world_with(common::Opts { repo: true, settings: serde_json::json!({"worker.agent":"codex", "reviewer.agent":"codex", "review.when":"always"}), ..Default::default() });
    w.script(serde_json::json!({"worker":[{"files":{"hello.txt":"hello"},"note":"Done."}],"reviewer":[{"verdict":{"meets":true,"text":"Pass.","findings":[]}}]}));
    let id = w.add("Trial work");
    let control = w.base.join("controller");
    let log = w.base.join("leases");
    std::fs::write(&control, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = repair ]; then echo REPAIR; else echo 'LEASE lease-1'; fi\n", log.display())).unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = sw::budget_binding::path();
    let saved = serde_json::json!({"approval":"/approval","database":"/db","control":control,"gate":control,"config":sw::store::files::config_dir(),"reports":w.base.join("reports"),"trial":{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}});
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let _binding_env = TrialEnv::install(&file);
    common::delegate(&id);
    let t = common::settle(&id);
    if t.s("stage") == "nosummary" { sw::actions::finish_line::finish_anyway(&id).unwrap(); }
    let t = common::settle(&id);
    let calls = std::fs::read_to_string(log).unwrap();
    assert!(calls.contains("--role implementation"), "{calls}");
    assert!(calls.contains("--role qa"), "{calls}\n{}", serde_json::Value::Object(t.clone()));
    let launch_calls = w.state().arr("calls").iter().map(text).filter(|c| c.starts_with("agent start") && c.contains("--kind codex")).collect::<Vec<_>>();
    assert!(launch_calls.len() >= 2, "{launch_calls:?}");
    assert!(launch_calls.iter().all(|c| c.contains("-m gpt-6.1-sol")), "{launch_calls:?}");
}
