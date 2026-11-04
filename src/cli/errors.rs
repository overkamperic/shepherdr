//! CLI errors: stable machine codes for JSON callers, the existing repo-tagged sentence for people.
use crate::err::{Error, Kind};
use crate::store::keys::{repo_keys, say_ids};

pub fn say(error: &Error, json: bool) -> String {
    let message = say_ids(&error.msg, &repo_keys());
    if !json { return message; }
    let code = match error.kind { Kind::OwnershipBusy => "ownership_busy", Kind::Busy => "busy", Kind::Refused => "refused", Kind::Plain => "error" };
    serde_json::json!({ "error": { "code": code, "message": message } }).to_string()
}
