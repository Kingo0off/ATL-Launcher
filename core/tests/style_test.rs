use friendlauncher_core::settings::clean_theme;
use serde_json::json;

#[test]
fn known_styles_are_kept() {
    for s in ["", "xp", "win98", "terminal", "minecraft", "aero"] {
        let t = clean_theme(&json!({ "style": s })).unwrap();
        assert_eq!(t["style"], s);
    }
}

#[test]
fn unknown_or_non_string_style_is_rejected() {
    assert!(clean_theme(&json!({ "style": "evil\"><script>" })).is_err());
    assert!(clean_theme(&json!({ "style": 5 })).is_err());
}

#[test]
fn style_can_be_combined_with_event() {
    let t = clean_theme(&json!({ "style": "xp", "event": "newyear" })).unwrap();
    assert_eq!(t["style"], "xp");
    assert_eq!(t["event"], "newyear");
}

#[test]
fn ui_state_keys_are_validated() {
    let ok = clean_theme(&json!({"sound": "off", "news_seen": "3.1.44", "br_limit": "50"})).unwrap();
    assert_eq!(ok["sound"], "off");
    assert_eq!(ok["news_seen"], "3.1.44");
    assert_eq!(ok["br_limit"], "50");
    assert!(clean_theme(&json!({"sound": "loud"})).is_err());
    assert!(clean_theme(&json!({"news_seen": "<b>"})).is_err());
    assert!(clean_theme(&json!({"br_limit": "7"})).is_err());
}

#[test]
fn news_drops_drafts_and_service_lines() {
    use friendlauncher_core::selfupdate::parse_news;
    let js = json!([
        {"tag_name": "v3.1.44", "name": "Voxel 3.1.44", "published_at": "2026-11-24T10:00:00Z", "html_url": "https://github.com/x/y/releases/tag/v3.1.44",
         "body": "Звуки и новости\n\n- Добавлены звуки\n\nCo-Authored-By: Claude <x@y>\nClaude-Session: https://claude.ai/code/s"},
        {"tag_name": "v3.1.43", "draft": true, "body": "черновик"},
        {"tag_name": "v3.1.42", "body": null}
    ]);
    let n = parse_news(&js);
    // черновик и релиз без описания в новости не попадают
    assert_eq!(n.len(), 1);
    assert_eq!(n[0]["version"], "3.1.44");
    assert_eq!(n[0]["date"], "2026-11-24");
    let body = n[0]["body"].as_str().unwrap();
    assert!(body.contains("Добавлены звуки"));
    assert!(!body.contains("Co-Authored") && !body.contains("Claude-Session"));
}

#[test]
fn update_notes_have_no_service_lines() {
    use friendlauncher_core::selfupdate::clean_body;
    let t = clean_body("Звуки\n\n- пункт\n\nCo-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>\nClaude-Session: https://claude.ai/code/x\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n");
    assert_eq!(t, "Звуки\n\n- пункт");
}
