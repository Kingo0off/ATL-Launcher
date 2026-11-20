use crate::util::{read_json, write_json};
use serde_json::Value;
use std::path::Path;

pub fn load(root: &Path) -> Vec<Value> {
    read_json(&root.join("accounts.json")).and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

pub fn save(root: &Path, accounts: &[Value]) -> std::io::Result<()> {
    write_json(&root.join("accounts.json"), &Value::Array(accounts.to_vec()))
}

pub fn remove(root: &Path, uuid: &str, kind: &str) -> std::io::Result<()> {
    let rest: Vec<Value> = load(root)
        .into_iter()
        .filter(|a| !(a["uuid"] == uuid && a["type"] == kind))
        .collect();
    save(root, &rest)
}
