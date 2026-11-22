use friendlauncher_core::api::valid_link;
use friendlauncher_core::catalog::trim_project;
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
