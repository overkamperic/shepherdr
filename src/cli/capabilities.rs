//! Read-only capability discovery for callers that need the legacy board's completion and ownership contracts.
pub fn say() {
    println!("{}", serde_json::json!({"schema": 1, "product": "herdr-projects",
        "budget_binding_loaded": std::env::var("CODEX_REQUEST_GATE_REQUIRED").as_deref() == Ok("1") && std::env::var_os("PIPELINE_BUDGET_MANIFEST").is_some(), "capabilities": {
        "worker_end_recovery": 1, "ownership_busy": 1, "bounded_launch": 1, "budget_refresh": 1
    }}));
}
