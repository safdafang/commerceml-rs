//! Builds an order the way a website hands it to 1C (`type=sale&mode=query`)
//! and prints it as windows-1251 or UTF-8 XML.
//!
//! ```sh
//! cargo run -p commerceml --example build_order
//! cargo run -p commerceml --example build_order -- --cp1251 > orders.xml
//! ```

use commerceml::{
    AddressField, AddressFieldType, CommercialInfo, Contact, ContactType, Counterparty, Document,
    DocumentItem, Encoding, Requisite, Role, WriteOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut order = Document::order("site-1001", "1001", "2026-11-02");
    order.time = Some("12:30:00".into());
    order.currency = Some("RUB".into());
    order.comment = Some("Доставка после 18:00".into());

    order.counterparties.push(Counterparty {
        id: Some("customer-42".into()),
        name: Some("Тест Тестов".into()),
        full_name: Some("Тестов Тест Тестович".into()),
        last_name: Some("Тестов".into()),
        first_name: Some("Тест".into()),
        role: Some(Role::Buyer),
        registration_address: Some(commerceml::Address {
            presentation: "000000, г. Примерный, ул. Тестовая, 1".into(),
            comment: None,
            fields: vec![AddressField {
                kind: AddressFieldType::PostalCode,
                value: "000000".into(),
            }],
        }),
        contacts: vec![Contact {
            kind: ContactType::Email,
            value: "customer@example.invalid".into(),
            comment: None,
        }],
        ..Counterparty::default()
    });

    order
        .items
        .push(DocumentItem::new("p-chair", "Стул складной", "2", "999.50")?);
    let mut delivery = DocumentItem::new("ORDER_DELIVERY", "Доставка заказа", "1", "348.00")?;
    delivery
        .product
        .requisites
        .push(Requisite::new("ВидНоменклатуры", "Услуга"));
    order.items.push(delivery);
    order.amount = order.items_total();

    order.requisites = vec![
        Requisite::new("Метод оплаты", "Банковская карта"),
        Requisite::new("Заказ оплачен", "true"),
        Requisite::new("Статус заказа", "[N] Принят"),
    ];

    let mut info = CommercialInfo::new("2026-11-02T12:31:00");
    info.documents.push(order);

    let options = if std::env::args().any(|a| a == "--cp1251") {
        WriteOptions::default().encoding(Encoding::WINDOWS_1251)
    } else {
        WriteOptions::default()
    };
    let bytes = options.to_vec(&info)?;

    assert_eq!(commerceml::from_slice(&bytes)?.info, info);
    std::io::Write::write_all(&mut std::io::stdout(), &bytes)?;
    Ok(())
}
