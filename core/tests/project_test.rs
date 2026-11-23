use friendlauncher_core::api::valid_link;
use friendlauncher_core::catalog::{cf_trim_files, cf_trim_project, trim_project};
use serde_json::json;

#[test]
fn project_is_trimmed_for_the_page() {
    let p = json!({
        "id": "P1", "slug": "fabric-api", "title": "Fabric API", "description": "d", "body": "# Fabric API\n\ntext",
        "icon_url": "https://cdn/x.png", "downloads": 291000000, "followers": 38000,
        "categories": ["library", "fabric"], "additional_categories": ["utility", "library"], "loaders": ["fabric", "quilt"],
        "game_versions": ["1.20.1", "1.21"], "client_side": "required", "server_side": "required",
        "gallery": [{"url": "https://cdn/g1.png", "title": "A", "description": null, "featured": true}, {"title": "без адреса"}],
        "license": {"id": "MIT", "name": "MIT License"}, "source_url": "https://github.com/x", "issues_url": null,
        "published": "2020-01-01T00:00:00Z", "updated": "2024-01-01T00:00:00Z", "project_type": "mod"
    });
    let t = trim_project(&p);
    assert_eq!(t["title"], "Fabric API");
    assert_eq!(t["body"], "# Fabric API\n\ntext");
    // названия загрузчиков не дублируются в категориях, повторы убраны
    assert_eq!(t["categories"], json!(["library", "utility"]));
    assert_eq!(t["loaders"], json!(["fabric", "quilt"]));
    assert_eq!(t["gallery"].as_array().unwrap().len(), 1);
    assert_eq!(t["gallery"][0]["description"], "");
    assert_eq!(t["license"], "MIT License");
    assert_eq!(t["links"]["source"], "https://github.com/x");
    assert_eq!(t["links"]["issues"], "");
    assert_eq!(t["downloads"], 291000000);
}

#[test]
fn only_plain_https_links_open() {
    assert!(valid_link("https://github.com/FabricMC/fabric"));
    assert!(valid_link("https://discord.gg/abc?x=1&y=2"));
    for bad in ["http://x.com", "file:///C:/Windows/System32/calc.exe", "javascript:alert(1)", "https://", "https://a b", "https://a\nb", "ftp://x", "", "\\\\server\\share"] {
        assert!(!valid_link(bad), "{:?}", bad);
    }
}

#[test]
fn curseforge_project_is_trimmed_like_modrinth() {
    let m = json!({
        "id": 306612, "slug": "fabric-api", "name": "Fabric API", "summary": "s", "classId": 6, "downloadCount": 1.5e8, "thumbsUpCount": 77,
        "logo": {"url": "https://media/x.png"}, "categories": [{"name": "Library"}, {"name": "Utility"}],
        "screenshots": [{"url": "https://media/s.png", "title": "T", "description": null}, {"title": "нет адреса"}],
        "latestFilesIndexes": [{"gameVersion": "1.20.1", "modLoader": 4}, {"gameVersion": "1.9.4", "modLoader": 1}, {"gameVersion": "1.20.1", "modLoader": 4}, {"gameVersion": "Fabric"}, {"gameVersion": "1.21"}],
        "links": {"websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/fabric-api", "sourceUrl": "https://github.com/x", "issuesUrl": null},
        "dateCreated": "2020-01-01T00:00:00Z", "dateModified": "2024-01-01T00:00:00Z"
    });
    let t = cf_trim_project(&m, "<p>Привет</p>");
    assert_eq!(t["project_id"], "cf:306612");
    assert_eq!(t["body_html"], "<p>Привет</p>");
    assert_eq!(t["categories"], json!(["Library", "Utility"]));
    // версии по возрастанию (1.9.4 раньше 1.20.1), без повторов и без названий загрузчиков
    assert_eq!(t["game_versions"], json!(["1.9.4", "1.20.1", "1.21"]));
    assert_eq!(t["loaders"], json!(["fabric", "forge"]));
    assert_eq!(t["gallery"].as_array().unwrap().len(), 1);
    assert_eq!(t["gallery"][0]["description"], "");
    assert_eq!(t["links"]["source"], "https://github.com/x");
    assert_eq!(t["links"]["issues"], "");
    assert_eq!(t["downloads"], 150000000);
    assert_eq!(t["project_type"], "mod");
}

#[test]
fn curseforge_files_are_trimmed_for_versions_tab() {
    let f = vec![
        json!({"id": 1, "displayName": "fabric-api-0.92", "fileName": "a.jar", "releaseType": 2, "gameVersions": ["1.20.1", "Fabric", "Client"], "downloadCount": 12.0, "fileDate": "2024-01-01T00:00:00Z"}),
        json!({"id": 2, "fileName": "b.jar", "releaseType": 1, "gameVersions": ["NeoForge", "1.21"]}),
    ];
    let t = cf_trim_files(&f);
    assert_eq!(t[0]["version_type"], "beta");
    assert_eq!(t[0]["game_versions"], json!(["1.20.1"]));
    assert_eq!(t[0]["loaders"], json!(["fabric"]));
    assert_eq!(t[1]["version_type"], "release");
    assert_eq!(t[1]["version_number"], "b.jar");
    assert_eq!(t[1]["loaders"], json!(["neoforge"]));
}
