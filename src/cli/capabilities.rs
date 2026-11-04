//! Read-only capability discovery for callers that need the legacy board's completion and ownership contracts.
pub fn say() {
    println!("{}", serde_json::json!({"schema": 1, "product": "herdr-projects", "capabilities": {
        "worker_end_recovery": 1, "ownership_busy": 1
    }}));
}
