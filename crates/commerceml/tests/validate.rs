//! Semantic validation.

use commerceml::stream::StreamReader;
use commerceml::validate::{IssueCode, Severity, Validator, validate};
use commerceml::{ParseOptions, from_str};

fn issues(xml: &str) -> Vec<commerceml::validate::Issue> {
    validate(&from_str(xml).unwrap().info)
}

fn codes(xml: &str) -> Vec<(Severity, IssueCode)> {
    issues(xml).iter().map(|i| (i.severity, i.code)).collect()
}

const HEAD: &str = r#"<КоммерческаяИнформация ВерсияСхемы="2.10" ДатаФормирования="2026-11-02T10:00:00">"#;
const TAIL: &str = "</КоммерческаяИнформация>";

#[test]
fn fixtures_are_clean_except_known_problems() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in ["import_2_10.xml", "rests_2_09.xml", "documents_2_09_2_10.xml"] {
        let info = commerceml::from_path(root.join(name)).unwrap().info;
        let found = validate(&info);
        assert!(found.is_empty(), "{name}: {found:#?}");
    }
    let info = commerceml::from_path(root.join("orders_2_03.xml")).unwrap().info;
    let found: Vec<_> = validate(&info)
        .into_iter()
        .map(|i| (i.severity, i.code))
        .collect();
    assert_eq!(found, [(Severity::Warning, IssueCode::InvalidValue)]);
    let info = commerceml::from_path(root.join("offers_2_07.xml")).unwrap().info;
    let found: Vec<_> = validate(&info).into_iter().map(|i| i.code).collect();
    assert_eq!(found, [IssueCode::InvalidValue, IssueCode::MissingField]);
}

#[test]
fn header_checks() {
    let found = codes("<КоммерческаяИнформация ВерсияСхемы=\"3.1\" ДатаФормирования=\"02.11.2026\"/>");
    assert_eq!(
        found,
        [
            (Severity::Warning, IssueCode::InvalidValue),
            (Severity::Warning, IssueCode::UnsupportedVersion)
        ]
    );
    assert_eq!(
        codes("<КоммерческаяИнформация/>"),
        [(Severity::Error, IssueCode::MissingField)]
    );
    assert_eq!(
        codes(
            "<КоммерческаяИнформация ВерсияСхемы=\"2.04\" ДатаФормирования=\"2026-11-02T10:00:00+03:00\"/>"
        )
        .len(),
        0
    );
}

#[test]
fn classifier_and_catalog_checks() {
    let xml = format!(
        r"{HEAD}
        <Классификатор><Ид></Ид><Наименование>К</Наименование>
          <Группы>
            <Группа><Ид>g</Ид><Наименование>A</Наименование>
              <Группы><Группа><Ид>g</Ид><Наименование>B</Наименование></Группа></Группы>
            </Группа>
          </Группы>
          <Свойства>
            <Свойство><Ид>color</Ид><Наименование>Цвет</Наименование><ТипЗначений>Справочник</ТипЗначений>
              <ВариантыЗначений><Справочник><ИдЗначения>red</ИдЗначения><Значение>Красный</Значение></Справочник></ВариантыЗначений>
            </Свойство>
            <Свойство><Ид>color</Ид><Наименование/></Свойство>
          </Свойства>
        </Классификатор>
        <Каталог><Ид>c</Ид><Наименование>К</Наименование><Товары>
          <Товар><Ид>p</Ид><Наименование>T</Наименование>
            <Группы><Ид>g</Ид><Ид>missing</Ид></Группы>
            <ЗначенияСвойств>
              <ЗначенияСвойства><Ид>color</Ид><Значение>blue</Значение></ЗначенияСвойства>
              <ЗначенияСвойства><Ид>size</Ид><Значение>L</Значение></ЗначенияСвойства>
            </ЗначенияСвойств>
          </Товар>
          <Товар><Ид>p</Ид><Наименование></Наименование></Товар>
          <Товар><Наименование>Без идентификатора</Наименование></Товар>
        </Товары></Каталог>{TAIL}"
    );
    let found = issues(&xml);
    let text: Vec<String> = found.iter().map(ToString::to_string).collect();
    let has = |needle: &str| text.iter().any(|t| t.contains(needle));
    assert!(has("error[missing-field] Классификатор \"\": <Ид>"), "{text:#?}");
    assert!(has("error[duplicate-id] Группа \"g\""), "{text:#?}");
    assert!(has("error[duplicate-id] Свойство \"color\""), "{text:#?}");
    assert!(
        has("error[missing-field] Свойство \"color\": <Наименование>"),
        "{text:#?}"
    );
    assert!(has("error[duplicate-id] Товар \"p\""), "{text:#?}");
    assert!(
        has("error[missing-field] Товар \"p\": <Наименование>"),
        "{text:#?}"
    );
    assert!(has("no Ид, Штрихкод or Артикул"), "{text:#?}");
    assert!(has("refers to Группа \"missing\""), "{text:#?}");
    assert!(has("refers to Свойство \"size\""), "{text:#?}");
    assert!(has("value \"blue\" is not in the dictionary"), "{text:#?}");
    assert_eq!(found.len(), 10, "{text:#?}");
}

#[test]
fn offer_checks() {
    let xml = format!(
        r#"{HEAD}<ПакетПредложений><Наименование></Наименование><ИдКаталога>c</ИдКаталога>
          <ТипыЦен><ТипЦены><Ид>retail</Ид></ТипЦены></ТипыЦен>
          <Склады><Склад><Ид>w1</Ид></Склад></Склады>
          <Предложения>
            <Предложение><Ид>o</Ид><Наименование>O</Наименование>
              <Цены>
                <Цена><ИдТипаЦены>retail</ИдТипаЦены><ЦенаЗаЕдиницу>-5</ЦенаЗаЕдиницу></Цена>
                <Цена><ИдТипаЦены>vip</ИдТипаЦены><ЦенаЗаЕдиницу>5</ЦенаЗаЕдиницу></Цена>
              </Цены>
              <Склад ИдСклада="w2" КоличествоНаСкладе="1"/>
              <Остатки><Остаток><Склад><Ид>w3</Ид><Количество>1</Количество></Склад></Остаток></Остатки>
            </Предложение>
            <Предложение><Ид>o</Ид><Наименование>O</Наименование></Предложение>
          </Предложения></ПакетПредложений>{TAIL}"#
    );
    let text: Vec<String> = issues(&xml).iter().map(ToString::to_string).collect();
    let has = |needle: &str| text.iter().any(|t| t.contains(needle));
    assert!(has("ПакетПредложений \"\": <Наименование>"), "{text:#?}");
    assert!(has("negative price -5"), "{text:#?}");
    assert!(has("refers to ТипЦены \"vip\""), "{text:#?}");
    assert!(has("refers to Склад \"w2\""), "{text:#?}");
    assert!(has("refers to Склад \"w3\""), "{text:#?}");
    assert!(has("duplicate-id] Предложение \"o\""), "{text:#?}");
    assert_eq!(text.len(), 6, "{text:#?}");
}

#[test]
fn changes_and_document_checks() {
    let xml = format!(
        r"{HEAD}
        <ИзмененияПакетаПредложений><Ид>ch</Ид><Предложения>
          <Предложение><Количество>1</Количество></Предложение>
        </Предложения></ИзмененияПакетаПредложений>
        <Документ><Ид>d</Ид><Номер></Номер><Дата>02.11.2026</Дата>
          <Товары><Товар><Ид>x</Ид><Наименование></Наименование></Товар></Товары>
          <ПодчиненныеДокументы><ПодчиненныйДокумент><Ид>s</Ид><Номер>1</Номер><Дата>2026-11-02</Дата></ПодчиненныйДокумент></ПодчиненныеДокументы>
        </Документ>
        <Документ><Ид>d</Ид><Номер>2</Номер><Дата>2026-11-02</Дата><ХозОперация>Заказ товара</ХозОперация></Документ>
        {TAIL}"
    );
    let text: Vec<String> = issues(&xml).iter().map(ToString::to_string).collect();
    let has = |needle: &str| text.iter().any(|t| t.contains(needle));
    assert!(has("Предложение \"\": no Ид"), "{text:#?}");
    assert!(has("Документ \"d\": <Номер>"), "{text:#?}");
    assert!(has("Документ \"d\": <ХозОперация> is missing"), "{text:#?}");
    assert!(has("is not YYYY-MM-DD"), "{text:#?}");
    assert!(has("line \"x\" has no Наименование"), "{text:#?}");
    assert!(has("ПодчиненныйДокумент \"s\": <ХозОперация>"), "{text:#?}");
    assert!(has("duplicate-id] Документ \"d\""), "{text:#?}");
    assert_eq!(text.len(), 7, "{text:#?}");
}

#[test]
fn streaming_validator_matches_whole_document() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/offers_2_07.xml");
    let mut v = Validator::new();
    for event in StreamReader::open(&path, ParseOptions::default()).unwrap() {
        v.event(&event.unwrap());
    }
    let streamed = v.finish();
    let whole = validate(&commerceml::from_path(&path).unwrap().info);
    assert_eq!(streamed, whole);
}
