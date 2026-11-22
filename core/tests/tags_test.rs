use friendlauncher_core::catalog::{clean_tags, mr_facets};
use serde_json::json;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn tags_become_separate_and_conditions() {
    let f = mr_facets("mod", "1.20.1", "fabric", &v(&["adventure", "optimization"]));
    assert_eq!(f[0], json!(["project_type:mod"]));
    assert_eq!(f[1], json!(["versions:1.20.1"]));
    assert_eq!(f[2], json!(["categories:fabric"]));
    // каждый тег в своей группе: Modrinth ищет и «приключения», и «оптимизацию»
    assert_eq!(f[3], json!(["categories:adventure"]));
    assert_eq!(f[4], json!(["categories:optimization"]));
    assert_eq!(f.len(), 5);
}

#[test]
fn no_tags_keeps_old_behaviour() {
    let f = mr_facets("shader", "", "fabric", &[]);
    assert_eq!(f, vec![json!(["project_type:shader"])]);
}

#[test]
fn bad_tags_are_dropped() {
    let t = clean_tags(&v(&["Adventure", "adventure", " magic ", "a:b", "x\"y", "", "ok-1", "t1", "t2", "t3", "t4", "t5"]));
    assert_eq!(t, v(&["adventure", "magic", "ok-1", "t1", "t2", "t3"]));
}
