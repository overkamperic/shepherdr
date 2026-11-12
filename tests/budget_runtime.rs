//! Bounded launches keep their mandatory lease even when optional notification hooks are absent.
use sw::budget_runtime::mandatory_args;
use crate::common;

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
