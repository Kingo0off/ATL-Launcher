//! Списки версий загрузчиков: Fabric, Quilt (meta API), Forge и NeoForge (maven-metadata.xml).
use crate::net::{get_json, get_text, loader_metadata, Endpoints, NetError};

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

/// Для Minecraft 1.20.1 NeoForge выпускался под именем net.neoforged:forge (версии вида 1.20.1-47.1.x).
pub fn neoforge_is_legacy(mc: &str) -> bool {
    mc == "1.20.1"
}

/// NeoForge нумеруется по Minecraft: 1.21.1 → 21.1.x, 1.21 → 21.0.x, а новая схема 26.1 → 26.1.0.x.
pub fn neoforge_prefix(mc: &str) -> Option<String> {
    let base = mc.split('-').next().unwrap_or(mc);
    let parts: Vec<&str> = base.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(if parts[0] == "1" {
        format!("{}.{}.", parts[1], parts.get(2).copied().unwrap_or("0"))
    } else {
        format!("{}.{}.{}.", parts[0], parts[1], parts.get(2).copied().unwrap_or("0"))
    })
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
    // Снапшоты Minecraft (26.1-snapshot-1) имеют сборки с суффиксом +snapshot-1.
    let snap = mc.split_once("-snapshot-").map(|(_, n)| format!("+snapshot-{}", n));
    let mut found: Vec<String> = versions
        .iter()
        .filter(|v| v.starts_with(&pre))
        .filter(|v| match &snap {
            Some(s) => v.ends_with(s.as_str()),
            None => !v.contains("+snapshot"),
        })
        .cloned()
        .collect();
    found.reverse();
    let is_pre = |v: &String| v.contains("beta") || v.contains("alpha");
    let (beta, stable): (Vec<String>, Vec<String>) = found.into_iter().partition(is_pre);
    stable.into_iter().chain(beta).collect()
}

/// Список Prism содержит точную версию Minecraft для каждого релиза.
pub fn prism_neoforge_versions(data: &serde_json::Value, mc: &str) -> Vec<String> {
    let found: Vec<String> = data["versions"].as_array().map(|versions| versions.iter()
        .filter(|v| v["requires"].as_array().map(|a| a.iter().any(|r| r["uid"] == "net.minecraft" && r["equals"] == mc)).unwrap_or(false))
        .filter_map(|v| v["version"].as_str().filter(|s| !s.is_empty()).map(String::from))
        .collect()).unwrap_or_default();
    let (pre, stable): (Vec<String>, Vec<String>) = found.into_iter().partition(|v| v.contains("beta") || v.contains("alpha"));
    stable.into_iter().chain(pre).collect()
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
        "neoforge" => {
            // Отдельный источник помогает, если сервер Maven NeoForge недоступен.
            if let Ok(raw) = loader_metadata(&ep.neoforge_prism) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let versions = prism_neoforge_versions(&data, mc);
                    if !versions.is_empty() { return Ok(versions); }
                }
            }
            let meta = if neoforge_is_legacy(mc) { &ep.neoforge_legacy_meta } else { &ep.neoforge_meta };
            let versions = xml_versions(&loader_metadata(meta)?);
            Ok(if neoforge_is_legacy(mc) { filter_forge(&versions, mc) } else { filter_neoforge(&versions, mc) })
        },
        other => Err(NetError(format!("Неизвестный загрузчик {}", other))),
    }
}
