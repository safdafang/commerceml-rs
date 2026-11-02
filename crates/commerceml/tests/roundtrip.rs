//! parse → write → parse gives the same data, for every fixture and option.

use commerceml::{Encoding, WriteOptions};

const FIXTURES: &[&str] = &[
    "import_2_10.xml",
    "offers_2_07.xml",
    "rests_2_09.xml",
    "orders_2_03.xml",
    "documents_2_09_2_10.xml",
    "misc_extensions.xml",
];

fn read_fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(path).unwrap()
}

#[test]
fn every_fixture_round_trips() {
    let variants = [
        WriteOptions::default(),
        WriteOptions::default().indent(false),
        WriteOptions::default().encoding(Encoding::WINDOWS_1251),
        WriteOptions::default().namespace(None).declaration(false),
    ];
    for name in FIXTURES {
        let first = commerceml::from_slice(&read_fixture(name)).unwrap();
        for options in &variants {
            let written = options.to_vec(&first.info).unwrap();
            let second = commerceml::from_slice(&written).unwrap_or_else(|e| {
                panic!(
                    "{name} with {options:?}: {e}\n{}",
                    String::from_utf8_lossy(&written)
                )
            });
            assert_eq!(second.info, first.info, "{name} with {options:?}");
            let unknown = |p: &commerceml::Parsed| {
                p.warnings
                    .iter()
                    .filter(|w| matches!(w.kind, commerceml::WarningKind::UnknownElement { .. }))
                    .count()
            };
            assert_eq!(unknown(&second), unknown(&first), "{name} with {options:?}");
        }
    }
}

#[test]
fn writing_is_stable() {
    for name in FIXTURES {
        let info = commerceml::from_slice(&read_fixture(name)).unwrap().info;
        let once = commerceml::to_string(&info).unwrap();
        let twice = commerceml::to_string(&commerceml::from_str(&once).unwrap().info).unwrap();
        assert_eq!(once, twice, "{name}");
    }
}

#[test]
fn windows_1251_output_is_declared_and_decodable() {
    let info = commerceml::from_slice(&read_fixture("orders_2_03.xml"))
        .unwrap()
        .info;
    let bytes = WriteOptions::default()
        .encoding(Encoding::WINDOWS_1251)
        .to_vec(&info)
        .unwrap();
    assert!(bytes.starts_with(b"<?xml version=\"1.0\" encoding=\"windows-1251\"?>"));
    let (text, _, had_errors) = encoding_rs::WINDOWS_1251.decode(&bytes);
    assert!(!had_errors);
    assert!(text.contains("<ХозОперация>Заказ товара</ХозОперация>"));
}

#[test]
fn to_path_writes_a_file() {
    let info = commerceml::from_slice(&read_fixture("rests_2_09.xml"))
        .unwrap()
        .info;
    let path = std::env::temp_dir().join(format!("commerceml-roundtrip-{}.xml", std::process::id()));
    commerceml::to_path(&info, &path).unwrap();
    let back = commerceml::from_path(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(back.info, info);
}
