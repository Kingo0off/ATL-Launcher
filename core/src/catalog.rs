//! Каталоги Modrinth и CurseForge: поиск, установка с зависимостями, обновления.
use crate::instances::{load_track, loader_title, save_track};
use crate::net::{download, http_json, sha1_file, Endpoints};
use crate::util::{safe_name, urlencode};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::Path;

pub const CF_PREFIX: &str = "cf:";

fn base_name(n: &str) -> String {
    n.rsplit(['/', '\\']).next().unwrap_or(n).to_string()
}

fn clean(n: &str) -> Result<String, String> {
    let b = base_name(n);
    safe_name(&b)?;
    Ok(b)
}

fn query(params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    format!("?{}", params.iter().map(|(k, v)| format!("{}={}", k, urlencode(v))).collect::<Vec<_>>().join("&"))
}

fn dir_of(ptype: &str) -> &'static str {
    match ptype {
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        "datapack" => "datapacks",
        _ => "mods",
    }
}

// ================= Modrinth =================

fn mr_get(ep: &Endpoints, path: &str, params: &[(&str, String)]) -> Result<Value, String> {
    let (st, js) = http_json(&format!("{}{}{}", ep.modrinth, path, query(params)), None, None, &[], None).map_err(|e| e.0)?;
    if st != 200 {
        return Err(format!("Modrinth: HTTP {} {}", st, js["description"].as_str().or(js["error"].as_str()).unwrap_or("")).trim().to_string());
    }
    Ok(js)
}

pub fn mr_loaders(kind: &str) -> Vec<&'static str> {
    match kind {
        "fabric" => vec!["fabric"],
        "quilt" => vec!["quilt", "fabric"],
        "forge" => vec!["forge"],
        "neoforge" => vec!["neoforge"],
        _ => vec![],
    }
}

/// Теги поиска оставляем только безопасные (латиница, цифры, дефис), без повторов и не больше шести.
pub fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().to_lowercase();
        let ok = !t.is_empty() && t.len() <= 32 && t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if ok && !out.contains(&t) && out.len() < 6 {
            out.push(t);
        }
    }
    out
}

/// Условия поиска Modrinth. Каждый тег — отдельное условие, поэтому теги складываются через «И».
pub fn mr_facets(ptype: &str, mc: &str, loader: &str, tags: &[String]) -> Vec<Value> {
    let mut facets: Vec<Value> = vec![json!([format!("project_type:{}", ptype)])];
    if !mc.is_empty() {
        facets.push(json!([format!("versions:{}", mc)]));
    }
    if !loader.is_empty() && (ptype == "mod" || ptype == "modpack") {
        let ls = mr_loaders(loader);
        let ls: Vec<String> = if ls.is_empty() { vec![loader.to_string()] } else { ls.iter().map(|s| s.to_string()).collect() };
        facets.push(json!(ls.iter().map(|x| format!("categories:{}", x)).collect::<Vec<_>>()));
    }
    for t in clean_tags(tags) {
        facets.push(json!([format!("categories:{}", t)]));
    }
    facets
}

pub fn mr_search(ep: &Endpoints, q: &str, ptype: &str, mc: &str, loader: &str, tags: &[String], sort: &str, offset: u64, limit: u64) -> Result<Value, String> {
    let facets = mr_facets(ptype, mc, loader, tags);
    let data = mr_get(
        ep,
        "/search",
        &[("query", q.to_string()), ("facets", Value::Array(facets).to_string()), ("limit", limit.to_string()), ("offset", offset.to_string()), ("index", sort.to_string())],
    )?;
    let hits: Vec<Value> = data["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| {
                    let cats = if h["display_categories"].is_array() { &h["display_categories"] } else { &h["categories"] };
                    json!({
                        "project_id": h["project_id"], "slug": h["slug"], "title": h["title"],
                        "description": h["description"].as_str().unwrap_or(""), "icon_url": h["icon_url"].as_str().unwrap_or(""),
                        "downloads": h["downloads"].as_i64().unwrap_or(0), "follows": h["follows"].as_i64().unwrap_or(0),
                        "author": h["author"].as_str().unwrap_or(""),
                        "categories": cats.as_array().map(|c| c.iter().take(4).cloned().collect::<Vec<_>>()).unwrap_or_default(),
                        "project_type": h["project_type"].as_str().unwrap_or(ptype)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let total = data["total_hits"].as_u64().unwrap_or(hits.len() as u64);
    Ok(json!({"hits": hits, "total": total}))
}

fn str_list(v: &Value, max: usize) -> Vec<Value> {
    v.as_array().map(|a| a.iter().filter(|x| x.is_string()).take(max).cloned().collect()).unwrap_or_default()
}

/// Из ответа Modrinth `/project/<id>` оставляет то, что показывает страница мода.
pub fn trim_project(p: &Value) -> Value {
    let loaders = str_list(&p["loaders"], 12);
    let mut cats: Vec<Value> = Vec::new();
    for c in str_list(&p["categories"], 16).into_iter().chain(str_list(&p["additional_categories"], 16)) {
        if !loaders.contains(&c) && !cats.contains(&c) {
            cats.push(c);
        }
    }
    let gallery: Vec<Value> = p["gallery"]
        .as_array()
        .map(|g| {
            g.iter()
                .filter_map(|x| {
                    let url = x["url"].as_str()?;
                    Some(json!({"url": url, "title": x["title"].as_str().unwrap_or(""), "description": x["description"].as_str().unwrap_or(""), "featured": x["featured"].as_bool().unwrap_or(false)}))
                })
                .take(24)
                .collect()
        })
        .unwrap_or_default();
    json!({
        "project_id": p["id"], "slug": p["slug"], "title": p["title"].as_str().unwrap_or(""),
        "description": p["description"].as_str().unwrap_or(""), "body": p["body"].as_str().unwrap_or(""),
        "icon_url": p["icon_url"].as_str().unwrap_or(""), "downloads": p["downloads"].as_i64().unwrap_or(0),
        "followers": p["followers"].as_i64().unwrap_or(0), "categories": cats, "loaders": loaders,
        "game_versions": str_list(&p["game_versions"], 300), "gallery": gallery,
        "client_side": p["client_side"].as_str().unwrap_or(""), "server_side": p["server_side"].as_str().unwrap_or(""),
        "license": p["license"]["name"].as_str().or(p["license"]["id"].as_str()).unwrap_or(""),
        "links": {"source": p["source_url"].as_str().unwrap_or(""), "issues": p["issues_url"].as_str().unwrap_or(""), "wiki": p["wiki_url"].as_str().unwrap_or(""), "discord": p["discord_url"].as_str().unwrap_or("")},
        "published": p["published"].as_str().unwrap_or(""), "updated": p["updated"].as_str().unwrap_or(""),
        "project_type": p["project_type"].as_str().unwrap_or("mod")
    })
}

pub fn mr_project(ep: &Endpoints, id: &str) -> Result<Value, String> {
    safe_name(id)?;
    Ok(trim_project(&mr_get(ep, &format!("/project/{}", id), &[])?))
}

/// Список версий для вкладки «Версии»: не больше 40 штук, журнал изменений обрезан.
pub fn mr_version_list(ep: &Endpoints, pid: &str, mc: &str, loader: &str, ptype: &str) -> Result<Vec<Value>, String> {
    let vs = mr_versions(ep, pid, mc, loader, ptype)?;
    Ok(vs
        .iter()
        .take(40)
        .map(|v| {
            json!({
                "id": v["id"], "name": v["name"].as_str().unwrap_or(""), "version_number": v["version_number"].as_str().unwrap_or(""),
                "version_type": v["version_type"].as_str().unwrap_or("release"), "game_versions": str_list(&v["game_versions"], 40),
                "loaders": str_list(&v["loaders"], 8), "downloads": v["downloads"].as_i64().unwrap_or(0),
                "date_published": v["date_published"].as_str().unwrap_or(""),
                "changelog": v["changelog"].as_str().unwrap_or("").chars().take(3000).collect::<String>()
            })
        })
        .collect())
}

fn mr_versions(ep: &Endpoints, pid: &str, mc: &str, loader: &str, ptype: &str) -> Result<Vec<Value>, String> {
    safe_name(pid)?;
    let mut params: Vec<(&str, String)> = Vec::new();
    if !mc.is_empty() && ptype != "modpack" {
        params.push(("game_versions", json!([mc]).to_string()));
    }
    if !loader.is_empty() && ptype == "mod" {
        let ls = mr_loaders(loader);
        let v = if ls.is_empty() { json!([loader]) } else { json!(ls) };
        params.push(("loaders", v.to_string()));
    }
    Ok(mr_get(ep, &format!("/project/{}/version", pid), &params)?.as_array().cloned().unwrap_or_default())
}

fn pick_version(vs: &[Value]) -> Option<&Value> {
    vs.iter().find(|v| v["version_type"] == "release").or_else(|| vs.first())
}

fn primary_file(v: &Value) -> Option<&Value> {
    let files = v["files"].as_array()?;
    files.iter().find(|f| f["primary"] == true).or_else(|| files.first())
}

pub fn mr_install(ep: &Endpoints, gdir: &Path, mc: &str, loader: &str, pid: &str, ptype: &str, meta: &Value, with_deps: bool, seen: &mut HashSet<String>, depth: u32) -> Result<Vec<String>, String> {
    seen.insert(pid.to_string());
    let versions = mr_versions(ep, pid, mc, loader, ptype)?;
    let title = meta["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(pid).to_string();
    let v = pick_version(&versions).ok_or_else(|| format!("Для Minecraft {} ({}) у «{}» нет подходящей версии", mc, loader_title(loader), title))?.clone();
    let f = primary_file(&v).ok_or_else(|| format!("У версии {} нет файлов", v["version_number"]))?.clone();
    let folder = dir_of(ptype);
    let name = clean(f["filename"].as_str().unwrap_or(""))?;
    let dest = gdir.join(folder).join(&name);
    let sha = f["hashes"]["sha1"].as_str().map(String::from);
    download(f["url"].as_str().unwrap_or(""), &dest, sha.as_deref(), None, 3).map_err(|e| e.0)?;
    let mut track = load_track(gdir);
    track.insert(
        format!("{}/{}", folder, name),
        json!({
            "project_id": v["project_id"], "version_id": v["id"], "version_number": v["version_number"],
            "title": if title.is_empty() { name.clone() } else { title }, "icon_url": meta["icon_url"].as_str().unwrap_or(""),
            "sha1": sha.clone().unwrap_or_else(|| sha1_file(&dest).unwrap_or_default())
        }),
    );
    save_track(gdir, &track);
    let mut installed = vec![format!("{}/{}", folder, name)];
    if with_deps && depth < 4 {
        for dep in v["dependencies"].as_array().cloned().unwrap_or_default() {
            let dpid = match dep["project_id"].as_str() {
                Some(d) if dep["dependency_type"] == "required" && !seen.contains(d) => d.to_string(),
                _ => continue,
            };
            if load_track(gdir).values().any(|t| t["project_id"] == dpid.as_str()) {
                seen.insert(dpid);
                continue;
            }
            safe_name(&dpid)?;
            let info = mr_get(ep, &format!("/project/{}", dpid), &[])?;
            let sub = json!({"title": info["title"], "icon_url": info["icon_url"]});
            installed.extend(mr_install(ep, gdir, mc, loader, &dpid, info["project_type"].as_str().unwrap_or("mod"), &sub, true, seen, depth + 1)?);
        }
    }
    Ok(installed)
}

/// (url, имя файла, sha1) последней версии модпака.
pub fn mr_modpack_file(ep: &Endpoints, pid: &str) -> Result<(String, String, Option<String>), String> {
    let vs = mr_versions(ep, pid, "", "", "modpack")?;
    let v = pick_version(&vs).ok_or("У модпака нет версий")?;
    let f = primary_file(v).ok_or("У модпака нет файлов")?;
    Ok((f["url"].as_str().unwrap_or("").to_string(), clean(f["filename"].as_str().unwrap_or("pack.mrpack"))?, f["hashes"]["sha1"].as_str().map(String::from)))
}

/// Ищет по хешам более новые версии включённых модов.
pub fn mr_check_updates(ep: &Endpoints, gdir: &Path, mc: &str, loader: &str) -> Result<Vec<Value>, String> {
    let mdir = gdir.join("mods");
    let mut hashes: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&mdir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with(".jar") {
                if let Ok(h) = sha1_file(&e.path()) {
                    hashes.push((h, n));
                }
            }
        }
    }
    if hashes.is_empty() {
        return Ok(vec![]);
    }
    let mut body = json!({"hashes": hashes.iter().map(|h| h.0.clone()).collect::<Vec<_>>(), "algorithm": "sha1", "game_versions": [mc]});
    let ls = mr_loaders(loader);
    if !ls.is_empty() {
        body["loaders"] = json!(ls);
    }
    let (st, js) = http_json(&format!("{}/version_files/update", ep.modrinth), Some(&body), None, &[], None).map_err(|e| e.0)?;
    if st != 200 {
        return Err(format!("Modrinth: HTTP {}", st));
    }
    let mut out = Vec::new();
    for (h, file) in &hashes {
        let ver = &js[h.as_str()];
        if !ver.is_object() {
            continue;
        }
        let f = match primary_file(ver) {
            Some(f) => f,
            None => continue,
        };
        let new_hash = match f["hashes"]["sha1"].as_str() {
            Some(x) if x != h => x,
            _ => continue,
        };
        out.push(json!({
            "file": file, "project_id": ver["project_id"], "version_id": ver["id"], "version_number": ver["version_number"],
            "url": f["url"], "filename": base_name(f["filename"].as_str().unwrap_or("")), "sha1": new_hash
        }));
    }
    Ok(out)
}

pub fn mr_apply_update(gdir: &Path, item: &Value) -> Result<(), String> {
    let mdir = gdir.join("mods");
    let new_name = clean(item["filename"].as_str().unwrap_or(""))?;
    let old_name = clean(item["file"].as_str().unwrap_or(""))?;
    download(item["url"].as_str().unwrap_or(""), &mdir.join(&new_name), item["sha1"].as_str(), None, 3).map_err(|e| e.0)?;
    if new_name != old_name {
        let _ = std::fs::remove_file(mdir.join(&old_name));
    }
    let mut track = load_track(gdir);
    let mut entry = track.remove(&format!("mods/{}", old_name)).unwrap_or_else(|| json!({}));
    for k in ["project_id", "version_id", "version_number", "sha1"] {
        entry[k] = item[k].clone();
    }
    track.insert(format!("mods/{}", new_name), entry);
    save_track(gdir, &track);
    Ok(())
}

// ================= CurseForge =================

fn cf_class(ptype: &str) -> i64 {
    match ptype {
        "modpack" => 4471,
        "resourcepack" => 12,
        "shader" => 6552,
        "datapack" => 6945,
        _ => 6,
    }
}

fn cf_class_dir(class: i64) -> &'static str {
    match class {
        12 => "resourcepacks",
        6552 => "shaderpacks",
        6945 => "datapacks",
        _ => "mods",
    }
}

fn cf_loader(kind: &str) -> Option<i64> {
    Some(match kind {
        "forge" => 1,
        "fabric" => 4,
        "quilt" => 5,
        "neoforge" => 6,
        _ => return None,
    })
}

fn cf_call(ep: &Endpoints, key: &str, path: &str, params: &[(&str, String)], body: Option<&Value>) -> Result<Value, String> {
    if key.trim().is_empty() {
        return Err("Нужен ключ API CurseForge. Если у тебя есть свой (бесплатный, console.curseforge.com), вставь его в «Настройки → Аккаунты и сервисы».".into());
    }
    let (st, js) = http_json(&format!("{}{}{}", ep.curseforge, path, query(params)), body, None, &[("x-api-key", key.trim())], None).map_err(|e| e.0)?;
    if st == 401 || st == 403 {
        return Err("CurseForge не принял ключ API. Проверь его в «Настройках».".into());
    }
    if st != 200 {
        return Err(format!("CurseForge: HTTP {} {}", st, js["error"].as_str().or(js["_raw"].as_str()).unwrap_or("")).trim().to_string());
    }
    Ok(js)
}

fn cf_hit(m: &Value, ptype: &str) -> Value {
    let logo = &m["logo"];
    json!({
        "project_id": format!("{}{}", CF_PREFIX, m["id"]), "slug": m["slug"], "title": m["name"].as_str().unwrap_or(""),
        "description": m["summary"].as_str().unwrap_or(""),
        "icon_url": logo["thumbnailUrl"].as_str().or(logo["url"].as_str()).unwrap_or(""),
        "downloads": m["downloadCount"].as_f64().unwrap_or(0.0) as i64, "follows": m["thumbsUpCount"].as_i64().unwrap_or(0),
        "author": m["authors"][0]["name"].as_str().unwrap_or(""),
        "categories": m["categories"].as_array().map(|c| c.iter().take(4).map(|x| json!(x["name"].as_str().unwrap_or(""))).collect::<Vec<_>>()).unwrap_or_default(),
        "project_type": ptype, "site_url": m["links"]["websiteUrl"].as_str().unwrap_or(""),
        "blocked": m["allowModDistribution"] == false
    })
}

pub fn cf_search(ep: &Endpoints, key: &str, q: &str, ptype: &str, mc: &str, loader: &str, sort: &str, offset: u64, limit: u64) -> Result<Value, String> {
    let sort_field = match sort {
        "downloads" => 6,
        "newest" => 11,
        "updated" => 3,
        _ => 1,
    };
    let mut params: Vec<(&str, String)> = vec![
        ("gameId", "432".into()),
        ("classId", cf_class(ptype).to_string()),
        ("pageSize", limit.to_string()),
        ("index", offset.to_string()),
        ("sortField", sort_field.to_string()),
        ("sortOrder", "desc".into()),
    ];
    if !q.is_empty() {
        params.push(("searchFilter", q.to_string()));
    }
    if !mc.is_empty() {
        params.push(("gameVersion", mc.to_string()));
    }
    if let (Some(id), true) = (cf_loader(loader), ptype == "mod" || ptype == "modpack") {
        params.push(("modLoaderType", id.to_string()));
    }
    let js = cf_call(ep, key, "/mods/search", &params, None)?;
    let hits: Vec<Value> = js["data"].as_array().map(|a| a.iter().map(|m| cf_hit(m, ptype)).collect()).unwrap_or_default();
    let total = js["pagination"]["totalCount"].as_u64().unwrap_or(hits.len() as u64).min(10000 - limit);
    Ok(json!({"hits": hits, "total": total}))
}

fn cf_id(pid: &str) -> Result<i64, String> {
    pid.strip_prefix(CF_PREFIX).unwrap_or(pid).parse::<i64>().map_err(|_| "Неверный идентификатор проекта CurseForge".to_string())
}

fn cf_files(ep: &Endpoints, key: &str, mod_id: i64, mc: &str, loader: &str, ptype: &str) -> Result<Vec<Value>, String> {
    let mut params: Vec<(&str, String)> = vec![("pageSize", "50".into())];
    if !mc.is_empty() && ptype != "modpack" {
        params.push(("gameVersion", mc.to_string()));
    }
    if let (Some(id), true) = (cf_loader(loader), ptype == "mod") {
        params.push(("modLoaderType", id.to_string()));
    }
    let mut files = cf_call(ep, key, &format!("/mods/{}/files", mod_id), &params, None)?["data"].as_array().cloned().unwrap_or_default();
    files.sort_by(|a, b| b["fileDate"].as_str().unwrap_or("").cmp(a["fileDate"].as_str().unwrap_or("")));
    Ok(files)
}

fn cf_pick(files: &[Value]) -> Option<&Value> {
    files.iter().find(|f| f["releaseType"] == 1).or_else(|| files.first())
}

pub fn cf_sha1(f: &Value) -> Option<String> {
    f["hashes"].as_array()?.iter().find(|h| h["algo"] == 1).and_then(|h| h["value"].as_str().map(String::from))
}

fn need_url(f: &Value, title: &str, site: &str) -> Result<String, String> {
    match f["downloadUrl"].as_str().filter(|u| !u.is_empty()) {
        Some(u) => Ok(u.to_string()),
        None => Err(format!(
            "Автор «{}» запретил скачивание сторонними лаунчерами. Скачай файл на сайте CurseForge и добавь его вручную (вкладка «Моды» → «Добавить файл»).{}",
            title,
            if site.is_empty() { String::new() } else { format!(" Страница: {}", site) }
        )),
    }
}

pub fn cf_install(ep: &Endpoints, key: &str, gdir: &Path, mc: &str, loader: &str, pid: &str, ptype: &str, meta: &Value, with_deps: bool, seen: &mut HashSet<i64>, depth: u32) -> Result<Vec<String>, String> {
    let mod_id = cf_id(pid)?;
    seen.insert(mod_id);
    let title = meta["title"].as_str().filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| mod_id.to_string());
    let files = cf_files(ep, key, mod_id, mc, loader, ptype)?;
    let f = cf_pick(&files).ok_or_else(|| format!("Для Minecraft {} ({}) у «{}» нет подходящего файла", mc, loader_title(loader), title))?.clone();
    let url = need_url(&f, &title, meta["site_url"].as_str().unwrap_or(""))?;
    let folder = cf_class_dir(cf_class(ptype));
    let name = clean(f["fileName"].as_str().unwrap_or(""))?;
    let dest = gdir.join(folder).join(&name);
    let sha = cf_sha1(&f);
    download(&url, &dest, sha.as_deref(), None, 3).map_err(|e| e.0)?;
    let mut track = load_track(gdir);
    track.insert(
        format!("{}/{}", folder, name),
        json!({
            "project_id": format!("{}{}", CF_PREFIX, mod_id), "source": "curseforge", "version_id": f["id"].to_string(),
            "version_number": f["displayName"].as_str().unwrap_or(&name), "title": title,
            "icon_url": meta["icon_url"].as_str().unwrap_or(""), "sha1": sha.clone().unwrap_or_else(|| sha1_file(&dest).unwrap_or_default())
        }),
    );
    save_track(gdir, &track);
    let mut installed = vec![format!("{}/{}", folder, name)];
    if with_deps && depth < 4 {
        for dep in f["dependencies"].as_array().cloned().unwrap_or_default() {
            let did = match dep["modId"].as_i64() {
                Some(d) if dep["relationType"] == 3 && !seen.contains(&d) => d,
                _ => continue,
            };
            let dkey = format!("{}{}", CF_PREFIX, did);
            if load_track(gdir).values().any(|t| t["project_id"] == dkey.as_str()) {
                seen.insert(did);
                continue;
            }
            let info = cf_call(ep, key, &format!("/mods/{}", did), &[], None)?["data"].clone();
            let class = info["classId"].as_i64().unwrap_or(6);
            let dtype = ["mod", "resourcepack", "shader", "datapack"].into_iter().find(|t| cf_class(t) == class).unwrap_or("mod");
            let sub = json!({"title": info["name"], "icon_url": info["logo"]["thumbnailUrl"], "site_url": info["links"]["websiteUrl"].as_str().unwrap_or("")});
            installed.extend(cf_install(ep, key, gdir, mc, loader, &dkey, dtype, &sub, true, seen, depth + 1)?);
        }
    }
    Ok(installed)
}

pub fn cf_modpack_file(ep: &Endpoints, key: &str, pid: &str, title: &str, site: &str) -> Result<(String, String, Option<String>), String> {
    let files = cf_files(ep, key, cf_id(pid)?, "", "", "modpack")?;
    let f = cf_pick(&files).ok_or("У сборки нет файлов")?;
    let t = if title.is_empty() { "сборки" } else { title };
    Ok((need_url(f, t, site)?, clean(f["fileName"].as_str().unwrap_or("pack.zip"))?, cf_sha1(f)))
}

pub fn cf_lookup_files(ep: &Endpoints, key: &str, ids: &[i64]) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for chunk in ids.chunks(200) {
        for f in cf_call(ep, key, "/mods/files", &[], Some(&json!({"fileIds": chunk})))?["data"].as_array().cloned().unwrap_or_default() {
            out.insert(f["id"].to_string(), f);
        }
    }
    Ok(out)
}

pub fn cf_lookup_mods(ep: &Endpoints, key: &str, ids: &[i64]) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for chunk in ids.chunks(200) {
        for m in cf_call(ep, key, "/mods", &[], Some(&json!({"modIds": chunk})))?["data"].as_array().cloned().unwrap_or_default() {
            out.insert(m["id"].to_string(), m);
        }
    }
    Ok(out)
}

pub fn cf_dir_for(class: i64) -> &'static str {
    cf_class_dir(class)
}
