//! [real programs] Capability discovery is machine-readable before any startup migration or task access.
use crate::common;
use common::*;
use serde_json::{json, Value};

#[test]
fn real_programs_capabilities_are_read_only_even_with_an_old_install_present() {
    let w = world();
    let home = w.root.join("capability-home");
    std::fs::create_dir_all(home.join(".herdr-superwork")).unwrap();
    std::fs::write(home.join(".herdr-superwork/untouched"), "keep").unwrap();
    for args in [vec!["capabilities", "--json"], vec!["capabilities"], vec!["version", "--json"]] {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_herdr-projects")).args(args)
            .env_clear().env("HOME", &home).env("PATH", "/usr/bin:/bin").current_dir(&home).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v, json!({"schema": 1, "product": "herdr-projects", "capabilities": {
            "worker_end_recovery": 1, "ownership_busy": 1
        }}));
        assert!(o.stderr.is_empty());
        let dirs: Vec<_> = std::fs::read_dir(&home).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(dirs, vec![std::ffi::OsString::from(".herdr-superwork")], "discovery must not migrate or create state");
        assert_eq!(std::fs::read_to_string(home.join(".herdr-superwork/untouched")).unwrap(), "keep");
    }
}
