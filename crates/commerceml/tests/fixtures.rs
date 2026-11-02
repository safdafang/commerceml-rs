//! Behaviour on the synthetic fixtures in `tests/fixtures`.

use commerceml::ParseOptions;
use commerceml::stream::{Event, StreamReader};
use commerceml::{
    BusinessOperation, ContactType, Decimal, Encoding, ProductStatus, PropertyValueType, Role, WarningKind,
};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

#[test]
fn import_classifier_and_catalog() {
    let parsed = commerceml::from_path(fixture("import_2_10.xml")).unwrap();
    let info = &parsed.info;
    assert_eq!(info.schema_version, "2.10");
    assert_eq!(parsed.encoding, Encoding::UTF_8);

    let c = info.classifier.as_ref().unwrap();
    assert_eq!(c.owner.as_ref().unwrap().inn.as_deref(), Some("0000000000"));
    assert_eq!(c.groups.len(), 2);
    let ids: Vec<_> = c.all_groups().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["g-furniture", "g-chairs", "g-tables", "g-light"]);
    assert_eq!(c.group("g-chairs").unwrap().name, "Стулья & табуреты");
    assert_eq!(c.group("g-tables").unwrap().deletion_mark, Some(false));
    let color = c.property("prop-color").unwrap();
    assert_eq!(color.value_type, Some(PropertyValueType::Dictionary));
    assert_eq!(color.dictionary_value("color-oak"), Some("Дуб"));
    assert_eq!(c.price_types[0].taxes[0].included_in_amount, Some(true));
    assert_eq!(c.warehouses[0].id, "wh-main");
    assert_eq!(c.units[0].international_code.as_deref(), Some("PCE"));
    assert_eq!(c.categories[0].id, "g-furniture");

    let catalog = info.catalog.as_ref().unwrap();
    assert_eq!(catalog.changes_only, Some(false));
    let products = info.products();
    assert_eq!(products.len(), 3);

    let chair = &products[0];
    assert_eq!(chair.status, Some(ProductStatus::Changed));
    assert_eq!(chair.name, "Стул складной   ", "whitespace is kept");
    assert_eq!(chair.barcode.as_deref(), Some("4600000000017"));
    let unit = chair.base_unit.as_ref().unwrap();
    assert_eq!((unit.code.as_deref(), unit.text.as_deref()), (Some("796"), None));
    assert_eq!(unit.conversions[0].coefficient.as_deref(), Some("1"));
    assert_eq!(chair.group_ids, ["g-chairs"]);
    assert_eq!(chair.category_id.as_deref(), Some("g-furniture"));
    assert!(
        chair
            .description
            .as_deref()
            .unwrap()
            .contains("\nВторая строка описания с символами <>&\"'.")
    );
    assert_eq!(chair.pictures.len(), 2);
    assert_eq!(
        chair.manufacturer.as_ref().unwrap().name.as_deref(),
        Some("Фабрика «Тест»")
    );
    assert_eq!(chair.property_value("prop-color"), Some("color-oak"));
    assert_eq!(chair.requisite("ВидНоменклатуры"), Some("Товар"));
    assert_eq!(chair.characteristics[0].value, "Дерево");
    assert_eq!(chair.weight, Some(d("3.5")));
    assert_eq!(chair.extra.len(), 1);
    assert_eq!(chair.extra[0].xml, "<Бренд>ТестБренд</Бренд>");

    let kit = &products[1];
    assert_eq!(kit.article.as_deref(), Some(""), "empty element is Some(\"\")");
    assert_eq!(kit.description.as_deref(), Some(""));
    assert_eq!(kit.tax_rates[0].rate, "Без налога");
    assert_eq!(kit.components[0].quantity, Some(d("4")));
    assert_eq!(kit.components[0].product.id, "p-chair");
    assert_eq!(kit.analogs[0].catalog_id.as_deref(), Some("catalog-2"));

    assert!(products[2].is_deleted());

    assert_eq!(parsed.warnings.len(), 1, "{:?}", parsed.warnings);
    let w = &parsed.warnings[0];
    assert_eq!(
        w.kind,
        WarningKind::UnknownElement {
            name: "Бренд".into()
        }
    );
    assert!(w.path.ends_with("Каталог/Товары/Товар/Бренд"), "{}", w.path);
    assert!(w.position.line.is_some(), "line numbers are resolved for files");
}

#[test]
fn offers_with_attribute_stock() {
    let parsed = commerceml::from_path(fixture("offers_2_07.xml")).unwrap();
    let pkg = parsed.info.offer_package.as_ref().unwrap();
    assert_eq!(pkg.id.as_deref(), Some("catalog-1#"));
    assert_eq!(pkg.price_types.len(), 2);
    assert_eq!(pkg.warehouses.len(), 2);
    let offers = &pkg.offers;
    assert_eq!(offers.len(), 3);

    let chair = &offers[0];
    assert_eq!(
        chair.price("price-retail").unwrap().price_per_unit,
        Some(d("1499.00"))
    );
    assert_eq!(
        chair.price("price-retail").unwrap().unit.unit.as_deref(),
        Some("шт")
    );
    let wholesale = chair.price("price-wholesale").unwrap();
    assert_eq!(
        wholesale.price_per_unit,
        Some(d("1200.50")),
        "lenient: spaces and comma"
    );
    assert_eq!(wholesale.min_quantity, Some(d("10")));
    assert_eq!(chair.warehouse_stock.len(), 2);
    assert_eq!(chair.total_quantity(), Some(d("12")));

    let red = &offers[1];
    assert_eq!(
        (red.product_id(), red.characteristic_id()),
        ("p-lamp", Some("c-red"))
    );
    assert_eq!(red.quantity, Some(d("-1")));
    assert_eq!(offers[2].characteristic_id(), None);

    assert_eq!(offers[2].prices[0].price_per_unit, None);
    assert_eq!(parsed.warnings.len(), 1);
    assert!(
        matches!(&parsed.warnings[0].kind, WarningKind::InvalidValue { value, .. } if value == "не число")
    );
}

#[test]
fn rests_file() {
    let parsed = commerceml::from_path(fixture("rests_2_09.xml")).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let pkg = parsed.info.offer_package.as_ref().unwrap();
    assert_eq!(pkg.changes_only, Some(true));
    assert_eq!(pkg.offers[0].rests.len(), 2);
    assert_eq!(pkg.offers[0].total_quantity(), Some(d("7")));
    assert_eq!(pkg.offers[1].total_quantity(), Some(d("3")));
}

#[test]
fn order_from_site() {
    let parsed = commerceml::from_path(fixture("orders_2_03.xml")).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let doc = &parsed.info.documents[0];
    assert_eq!(doc.operation, Some(BusinessOperation::OrderGoods));
    assert_eq!(doc.amount, Some(d("2347")));
    assert_eq!(doc.items_total(), Some(d("2347.00")));
    assert_eq!(doc.comment.as_deref(), Some(""));
    let buyer = doc.counterparty(&Role::Buyer).unwrap();
    assert_eq!(buyer.id.as_deref(), Some("1#user#Тестов Тест"));
    assert_eq!(buyer.contacts[0].kind, ContactType::Email);
    assert_eq!(buyer.contacts[1].comment.as_deref(), Some("после 18:00"));
    assert_eq!(buyer.registration_address.as_ref().unwrap().fields.len(), 2);
    let rep = &buyer.representatives[0];
    assert_eq!(rep.relation, "Контактное лицо");
    assert_eq!(rep.counterparty.id.as_deref(), Some("contact-1"));
    let delivery = &doc.items[0];
    assert_eq!(delivery.product.id, "ORDER_DELIVERY");
    assert_eq!(
        delivery.product.base_unit.as_ref().unwrap().text.as_deref(),
        Some("шт")
    );
    assert_eq!(doc.items[1].discounts[0].included_in_amount, Some(true));
    assert_eq!(doc.requisite("Статус заказа"), Some("[N] Принят"));
}

#[test]
fn documents_2_09_and_2_10() {
    let parsed = commerceml::from_path(fixture("documents_2_09_2_10.xml")).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let doc = &parsed.info.documents[0];
    let buyer = &doc.counterparties[0];
    assert!(buyer.is_legal_entity());
    assert_eq!(buyer.head.as_ref().unwrap().position, "Генеральный директор");
    assert!(
        buyer.head.as_ref().unwrap().registration_address.is_some(),
        "XSD spelling АресРегистрации"
    );
    assert_eq!(buyer.bank_accounts[0].bank.bic.as_deref(), Some("000000000"));
    assert_eq!(buyer.warehouse.as_ref().unwrap().id, "buyer-wh");
    assert_eq!(doc.taxes[0].amount, Some(d("200")));
    assert_eq!(doc.extra_charges[0].percent.as_deref(), Some("0"));
    let item = &doc.items[0];
    assert_eq!(item.unit.unit.as_deref(), Some("704"));
    assert_eq!(
        item.customs_declaration.as_deref(),
        Some("00000000/000000/0000000")
    );
    assert_eq!(item.warehouses[0].quantity, Some(d("1")));
    assert_eq!(item.additional_requisites[0].name, "Сборка");
    assert_eq!(
        doc.signatories[0]
            .workplace
            .as_ref()
            .unwrap()
            .official_name
            .as_deref(),
        Some("ООО \"Тестовая организация\"")
    );
    assert_eq!(doc.payments[0].transaction_number, "tx-0001");
    assert_eq!(doc.subordinate_documents.len(), 2);
    assert_eq!(
        doc.subordinate_documents[1].operation,
        Some(BusinessOperation::ReleaseGoods)
    );
    assert_eq!(doc.number_1c.as_deref(), Some("ТД00-000077"));
}

#[test]
fn changes_counterparties_container_and_prefixes() {
    let parsed = commerceml::from_path(fixture("misc_extensions.xml")).unwrap();
    let info = &parsed.info;
    assert_eq!(info.schema_version, "2.08", "prefixed attribute");
    let changes = &info.offer_package_changes[0];
    assert_eq!(changes.changes_only, Some(true));
    let o = &changes.offers[0];
    assert_eq!(o.characteristic_id.as_deref(), Some("c-default"));
    assert_eq!(o.warehouse_stock[0].quantity, Some(d("5")));
    assert_eq!(o.prices[0].price_per_unit, Some(d("1399")));
    let c = &info.counterparties[0];
    assert_eq!(c.okpo.as_deref(), Some("00000000"), "КодПоОКПО alias");
    assert_eq!(c.version.as_deref(), Some("AAAAAgAAAAE="));
    assert_eq!(c.gender, Some(commerceml::Gender::Male));
    assert_eq!(info.documents[0].basis.as_deref(), Some("order-77"));
    assert_eq!(info.extra.len(), 1);
    assert_eq!(info.extra[0].name, "ПользовательскиеСправочники");
    assert_eq!(parsed.warnings.len(), 1);
}

#[test]
fn stream_and_full_parse_agree() {
    for name in ["import_2_10.xml", "offers_2_07.xml", "misc_extensions.xml"] {
        let mut events = Vec::new();
        for e in StreamReader::open(fixture(name), ParseOptions::default()).unwrap() {
            events.push(e.unwrap());
        }
        assert!(matches!(events[0], Event::Start(_)), "{name}");
        let products = events.iter().filter(|e| matches!(e, Event::Product(_))).count();
        let full = commerceml::from_path(fixture(name)).unwrap();
        assert_eq!(products, full.info.products().len(), "{name}");
    }
}

#[test]
fn stream_section_headers() {
    let reader = StreamReader::open(fixture("import_2_10.xml"), ParseOptions::default()).unwrap();
    let events: Vec<Event> = reader.map(Result::unwrap).collect();
    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::ClassifierStart(c) => Some(c.name.clone()),
            Event::CatalogStart(c) => Some(c.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(starts, ["Классификатор (Тестовый магазин)", "Основной каталог"]);
    let end = events.iter().find_map(|e| match e {
        Event::ClassifierEnd(c) => Some(c),
        _ => None,
    });
    let end = end.unwrap();
    assert!(end.groups.is_empty() && end.owner.is_some());
    let groups = events.iter().filter(|e| matches!(e, Event::Group(_))).count();
    assert_eq!(groups, 2, "only top-level groups are separate events");
}

#[test]
fn windows_1251_input() {
    let utf8 = std::fs::read_to_string(fixture("import_2_10.xml")).unwrap();
    let decl = utf8.replace("encoding=\"UTF-8\"", "encoding=\"windows-1251\"");
    let (bytes, _, unmappable) = encoding_rs::WINDOWS_1251.encode(&decl);
    assert!(!unmappable);
    let parsed = commerceml::from_slice(&bytes).unwrap();
    assert_eq!(parsed.encoding, Encoding::WINDOWS_1251);
    let reference = commerceml::from_str(&utf8).unwrap();
    assert_eq!(parsed.info, reference.info);
}
