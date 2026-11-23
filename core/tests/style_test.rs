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
