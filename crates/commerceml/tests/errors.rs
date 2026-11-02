//! Fatal errors, strict mode and positions.

use commerceml::{Error, ParseOptions, WarningKind};

#[test]
fn malformed_xml_reports_line_and_column() {
    let xml = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\">\n  <Каталог>\n    <Ид>1</Ид>\n  </Классификатор>\n</КоммерческаяИнформация>";
    let err = commerceml::from_str(xml).unwrap_err();
    let Error::Xml { position, .. } = &err else {
        panic!("unexpected error {err:?}");
    };
    assert_eq!(position.line, Some(4), "{err}");
    assert!(err.to_string().contains("line 4"), "{err}");
}

#[test]
fn wrong_root_is_an_error() {
    let err = commerceml::from_str("<?xml version=\"1.0\"?>\n<Каталог/>").unwrap_err();
    assert!(
        matches!(&err, Error::NotCommerceMl { found, .. } if found == "Каталог"),
        "{err}"
    );
    assert_eq!(err.position().unwrap().line, Some(2));
}

#[test]
fn empty_and_truncated_input() {
    assert!(matches!(commerceml::from_str(""), Err(Error::Xml { .. })));
    assert!(matches!(
        commerceml::from_str("<!-- nothing -->"),
        Err(Error::Xml { .. })
    ));
    let truncated =
        "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"x\"><Каталог><Товары><Товар><Ид>1";
    let err = commerceml::from_str(truncated).unwrap_err();
    assert!(matches!(err, Error::Xml { .. }), "{err}");
}

#[test]
fn unsupported_encoding() {
    let err = commerceml::from_slice(b"<?xml version=\"1.0\" encoding=\"x-klingon\"?><a/>").unwrap_err();
    assert!(matches!(err, Error::UnsupportedEncoding(ref e) if e == "x-klingon"));
}

#[test]
fn self_closing_root() {
    let parsed = commerceml::from_str(
        "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\"/>",
    )
    .unwrap();
    assert!(parsed.info.catalog.is_none());
    assert!(parsed.warnings.is_empty());
}

#[test]
fn missing_root_attributes_and_unknown_attributes_warn() {
    let parsed = commerceml::from_str(
        "<КоммерческаяИнформация Лишний=\"1\"><Каталог Тоже=\"2\"/></КоммерческаяИнформация>",
    )
    .unwrap();
    let kinds: Vec<_> = parsed.warnings.iter().map(|w| w.kind.clone()).collect();
    assert!(kinds.contains(&WarningKind::UnknownAttribute {
        name: "Лишний".into()
    }));
    assert!(kinds.contains(&WarningKind::UnknownAttribute {
        name: "Тоже".into()
    }));
    assert!(kinds.contains(&WarningKind::MissingElement {
        name: "ВерсияСхемы".into()
    }));
    assert!(kinds.contains(&WarningKind::MissingElement {
        name: "ДатаФормирования".into()
    }));
    assert!(parsed.info.catalog.is_some());
}

#[test]
fn strict_mode_fails_on_first_warning() {
    let xml = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\">\n<Каталог><Ид>1</Ид><Наименование>К</Наименование><Товары>\n<Товар><Наименование>T</Наименование><Бренд>X</Бренд></Товар></Товары></Каталог></КоммерческаяИнформация>";
    assert_eq!(commerceml::from_str(xml).unwrap().warnings.len(), 1);
    let err = ParseOptions::default().strict(true).parse_str(xml).unwrap_err();
    let Error::Strict(w) = &err else { panic!("{err:?}") };
    assert_eq!(
        w.kind,
        WarningKind::UnknownElement {
            name: "Бренд".into()
        }
    );
    assert_eq!(w.position.line, Some(3));
}

#[test]
fn unknown_elements_can_be_dropped() {
    let xml = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\"><Каталог><Товары><Товар><Наименование>T</Наименование><Бренд>X</Бренд></Товар></Товары></Каталог><Лишнее/></КоммерческаяИнформация>";
    let kept = commerceml::from_str(xml).unwrap();
    assert_eq!(kept.info.products()[0].extra.len(), 1);
    assert_eq!(kept.info.extra.len(), 1);
    let dropped = ParseOptions::default()
        .keep_unknown(false)
        .parse_str(xml)
        .unwrap();
    assert!(dropped.info.products()[0].extra.is_empty());
    assert!(dropped.info.extra.is_empty());
    assert_eq!(dropped.warnings.len(), 2);
}

#[test]
fn text_and_children_in_wrong_places() {
    let xml = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\">stray<Каталог><Наименование>A<b>B</b>C</Наименование><ИдКлассификатора>&unknown;</ИдКлассификатора></Каталог></КоммерческаяИнформация>";
    let parsed = commerceml::from_str(xml).unwrap();
    let kinds: Vec<_> = parsed.warnings.iter().map(|w| w.kind.clone()).collect();
    assert_eq!(kinds[0], WarningKind::UnexpectedText);
    assert_eq!(
        kinds[1],
        WarningKind::UnexpectedChildren {
            name: "Наименование".into()
        }
    );
    assert_eq!(
        kinds[2],
        WarningKind::UnknownEntity {
            name: "unknown".into()
        }
    );
    let catalog = parsed.info.catalog.unwrap();
    assert_eq!(catalog.name, "AC");
    assert_eq!(catalog.classifier_id.as_deref(), Some("&unknown;"));
}

#[test]
fn invalid_booleans_warn() {
    let xml = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\"><Каталог СодержитТолькоИзменения=\"может быть\"><Товары><Товар><Наименование>T</Наименование><ПометкаУдаления>Да</ПометкаУдаления></Товар></Товары></Каталог></КоммерческаяИнформация>";
    let parsed = commerceml::from_str(xml).unwrap();
    assert_eq!(parsed.warnings.len(), 2, "{:?}", parsed.warnings);
    assert_eq!(parsed.info.catalog.as_ref().unwrap().changes_only, None);
    assert_eq!(parsed.info.products()[0].deletion_mark, None);
}

#[test]
fn warning_limit() {
    let mut xml = String::from(
        "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\">",
    );
    xml.extend((0..20).map(|i| format!("<X{i}/>")));
    xml.push_str("</КоммерческаяИнформация>");
    let parsed = ParseOptions::default().max_warnings(5).parse_str(&xml).unwrap();
    assert_eq!(parsed.warnings.len(), 5);
    assert_eq!(parsed.suppressed_warnings, 15);
}

#[test]
fn forced_encoding() {
    let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(
        "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\"><Каталог><Наименование>Ёлка</Наименование></Каталог></КоммерческаяИнформация>",
    );
    assert!(commerceml::from_slice(&bytes).is_err());
    let parsed = ParseOptions::default()
        .encoding(Some(commerceml::Encoding::WINDOWS_1251))
        .parse_slice(&bytes)
        .unwrap();
    assert_eq!(parsed.info.catalog.unwrap().name, "Ёлка");
}

#[test]
fn characters_not_allowed_in_xml() {
    let head = "<КоммерческаяИнформация ВерсияСхемы=\"2.10\" ДатаФормирования=\"2026-01-01T00:00:00\">";
    let xml = format!(
        "{head}<Каталог Атр=\"a\u{1}b\"><Наименование>x\u{B}y&#1;z</Наименование></Каталог></КоммерческаяИнформация>"
    );
    let parsed = commerceml::from_str(&xml).unwrap();
    assert_eq!(parsed.info.catalog.unwrap().name, "xyz");
    let invalid = parsed
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::InvalidCharacter)
        .count();
    assert_eq!(
        invalid, 3,
        "attribute, raw control character, character reference: {:?}",
        parsed.warnings
    );
    let xml = format!("{head}<Ката\u{1}лог/></КоммерческаяИнформация>");
    assert!(matches!(commerceml::from_str(&xml), Err(Error::Xml { .. })));
}
