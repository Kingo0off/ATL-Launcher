//! Списки версий загрузчиков: Fabric, Quilt (meta API), Forge и NeoForge (maven-metadata.xml).
use crate::net::{get_json, get_text, Endpoints, NetError};

pub fn xml_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<version>") {
        let after = &rest[i + "<version>".len()..];
        match after.find("</version>") {
            Some(j) => {
                let v = after[..j].trim();
                if !v.is_empty() {
                    out.push(v.to_string());
                }
                rest = &after[j + "</version>".len()..];
            }
            None => break,
        }
    }
    out
}

/// NeoForge нумеруется как <minor>.<patch>.x, где MC = 1.<minor>.<patch>.
pub fn neoforge_prefix(mc: &str) -> Option<String> {
    let parts: Vec<&str> = mc.split('.').collect();
    if parts.len() < 2 || parts[0] != "1" {
        return None;
    }
    Some(format!("{}.{}.", parts[1], parts.get(2).copied().unwrap_or("0")))
}

pub fn filter_forge(versions: &[String], mc: &str) -> Vec<String> {
    let pre = format!("{}-", mc);
    let mut found: Vec<String> = versions.iter().filter_map(|v| v.strip_prefix(&pre).map(String::from)).collect();
    found.reverse();
    found
}

pub fn filter_neoforge(versions: &[String], mc: &str) -> Vec<String> {
    let pre = match neoforge_prefix(mc) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut found: Vec<String> = versions.iter().filter(|v| v.starts_with(&pre)).cloned().collect();
    found.reverse();
    let (beta, stable): (Vec<String>, Vec<String>) = found.into_iter().partition(|v| v.contains("beta"));
    stable.into_iter().chain(beta).collect()
}

/// Версии загрузчика для данной версии Minecraft, свежие первыми.
pub fn loader_versions(ep: &Endpoints, kind: &str, mc: &str) -> Result<Vec<String>, NetError> {
    match kind {
        "vanilla" => Ok(Vec::new()),
        "fabric" | "quilt" => {
            let base = if kind == "fabric" { &ep.fabric } else { &ep.quilt };
            let data = get_json(&format!("{}/versions/loader/{}", base, mc))?;
            let all: Vec<String> = data
                .as_array()
                .map(|a| a.iter().filter_map(|x| x["loader"]["version"].as_str().map(String::from)).collect())
                .unwrap_or_default();
            let is_pre = |v: &String| ["beta", "alpha", "pre"].iter().any(|p| v.contains(p));
            let (pre, stable): (Vec<String>, Vec<String>) = all.into_iter().partition(is_pre);
            Ok(stable.into_iter().chain(pre).collect())
        }
        "forge" => Ok(filter_forge(&xml_versions(&get_text(&ep.forge_meta)?), mc)),
        "neoforge" => Ok(filter_neoforge(&xml_versions(&get_text(&ep.neoforge_meta)?), mc)),
        other => Err(NetError(format!("Неизвестный загрузчик {}", other))),
    }
}
