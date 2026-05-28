use std::fs;
use std::path::{Path, PathBuf};

use engine::build_dispatch_bundle;
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine crate has workspace parent")
        .to_path_buf()
}

fn golden_paths() -> Vec<PathBuf> {
    let dir = repo_root().join("tests/fixtures/dispatch_wire/v1");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("dispatch wire fixture dir exists")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn rust_dispatch_bundle_matches_python_golden_fixtures() {
    let paths = golden_paths();
    assert_eq!(paths.len(), 20);

    for path in paths {
        let raw = fs::read_to_string(&path).expect("fixture is readable");
        let fixture: Value = serde_json::from_str(&raw).expect("fixture is json");
        let request = &fixture["request"];
        let actual = build_dispatch_bundle(
            request["raw_request"].as_str().expect("raw_request string"),
            request["request_source"]
                .as_str()
                .expect("request_source string"),
        );
        assert_eq!(actual, fixture["golden_bundle"], "fixture drift: {path:?}");
    }
}
