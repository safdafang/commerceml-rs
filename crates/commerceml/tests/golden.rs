//! Golden files: the parse result of every fixture, as JSON, is compared
//! with `tests/golden/<fixture>.json`.
//!
//! After an intended change, regenerate with
//! `UPDATE_GOLDEN=1 cargo test -p commerceml --features serde --test golden`
//! and review the diff.

use std::path::Path;

#[derive(serde::Serialize)]
struct Golden<'a> {
    info: &'a commerceml::CommercialInfo,
    warnings: Vec<String>,
}

#[test]
fn fixtures_match_golden_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("fixtures"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    fixtures.sort();
    assert_ne!(fixtures.len(), 0);
    let mut mismatched = Vec::new();
    for path in fixtures {
        let parsed = commerceml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let golden = Golden {
            info: &parsed.info,
            warnings: parsed.warnings.iter().map(ToString::to_string).collect(),
        };
        let json = serde_json::to_string_pretty(&golden).unwrap() + "\n";
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let golden_path = root.join("golden").join(format!("{name}.json"));
        if update {
            std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
            std::fs::write(&golden_path, &json).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_GOLDEN=1", golden_path.display()))
            .replace("\r\n", "\n");
        if expected != json {
            mismatched.push(name);
        }
    }
    assert!(
        mismatched.is_empty(),
        "golden mismatch for {mismatched:?}; rerun with UPDATE_GOLDEN=1 and review the diff"
    );
}

#[test]
fn json_round_trips_through_serde() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/documents_2_09_2_10.xml");
    let info = commerceml::from_path(path).unwrap().info;
    let json = serde_json::to_string(&info).unwrap();
    let back: commerceml::CommercialInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back, info);
}
