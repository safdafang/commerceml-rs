//! Property tests: any model value survives write → parse unchanged.

use commerceml::*;
use proptest::collection::vec;
use proptest::option::of;
use proptest::prelude::*;

/// Text with Cyrillic, XML special characters and whitespace (incl. `\r`).
fn text() -> impl Strategy<Value = String> {
    "[a-zA-Zа-яА-ЯёЁ0-9 <>&'\"#.,:;/\\-\r\n\t]{0,12}"
}

/// Non-empty text without surrounding whitespace (for trimmed fields).
fn word() -> impl Strategy<Value = String> {
    "[a-zA-Zа-яА-Я0-9][a-zA-Zа-яА-Я0-9 <>&-]{0,8}[a-zA-Zа-яА-Я0-9]"
}

fn opt_text() -> impl Strategy<Value = Option<String>> {
    of(text())
}

fn decimal() -> impl Strategy<Value = Decimal> {
    (any::<i64>(), 0u32..=6).prop_map(|(m, s)| Decimal::new(i128::from(m), s))
}

fn opt_decimal() -> impl Strategy<Value = Option<Decimal>> {
    of(decimal())
}

fn opt_bool() -> impl Strategy<Value = Option<bool>> {
    of(any::<bool>())
}

/// A value outside every enumeration (prefixed so it never collides).
fn other() -> impl Strategy<Value = String> {
    "x-[a-zа-я]{0,6}"
}

fn role() -> BoxedStrategy<Role> {
    prop_oneof![
        Just(Role::Seller),
        Just(Role::Buyer),
        Just(Role::Licensee),
        other().prop_map(Role::Other),
    ]
    .boxed()
}

fn contact() -> BoxedStrategy<Contact> {
    (
        prop_oneof![
            Just(ContactType::Email),
            Just(ContactType::PhoneWork),
            other().prop_map(ContactType::Other)
        ],
        text(),
        opt_text(),
    )
        .prop_map(|(kind, value, comment)| Contact { kind, value, comment })
        .boxed()
}

fn address() -> BoxedStrategy<Address> {
    (
        text(),
        opt_text(),
        vec(
            (
                prop_oneof![
                    Just(AddressFieldType::City),
                    other().prop_map(AddressFieldType::Other)
                ],
                text(),
            )
                .prop_map(|(kind, value)| AddressField { kind, value }),
            0..3,
        ),
    )
        .prop_map(|(presentation, comment, fields)| Address {
            presentation,
            comment,
            fields,
        })
        .boxed()
}

fn requisite() -> BoxedStrategy<Requisite> {
    (text(), vec(text(), 0..3))
        .prop_map(|(name, values)| Requisite { name, values })
        .boxed()
}

fn warehouse() -> BoxedStrategy<Warehouse> {
    (
        text(),
        opt_text(),
        opt_text(),
        of(address()),
        vec(contact(), 0..2),
        opt_bool(),
        opt_text(),
    )
        .prop_map(
            |(id, name, comment, address, contacts, deletion_mark, version)| Warehouse {
                id,
                name,
                comment,
                address,
                contacts,
                deletion_mark,
                version,
                extra: Vec::new(),
            },
        )
        .boxed()
}

fn bank() -> BoxedStrategy<Bank> {
    (opt_text(), opt_text(), opt_text(), opt_text())
        .prop_map(|(correspondent_account, name, bic, swift)| Bank {
            correspondent_account,
            name,
            bic,
            swift,
            ..Bank::default()
        })
        .boxed()
}

fn bank_account() -> BoxedStrategy<BankAccount> {
    (text(), bank(), of(bank()), opt_text())
        .prop_map(|(number, bank, correspondent_bank, comment)| BankAccount {
            number,
            bank,
            correspondent_bank,
            comment,
        })
        .boxed()
}

/// A counterparty without nested counterparties.
fn simple_counterparty() -> BoxedStrategy<Counterparty> {
    (
        (
            opt_text(),
            opt_text(),
            opt_text(),
            of(address()),
            opt_text(),
            opt_text(),
        ),
        (opt_text(), opt_text(), opt_text(), opt_text(), opt_text()),
        (
            opt_text(),
            of(prop_oneof![Just(Gender::Male), Just(Gender::Female)]),
            of(address()),
        ),
        (
            opt_text(),
            vec(contact(), 0..2),
            of(role()),
            opt_bool(),
            opt_text(),
        ),
        vec(bank_account(), 0..2),
    )
        .prop_map(|(a, b, c, d, accounts)| Counterparty {
            id: a.0,
            name: a.1,
            official_name: a.2,
            legal_address: a.3,
            inn: a.4,
            kpp: a.5,
            okpo: b.0,
            full_name: b.1,
            last_name: b.2,
            first_name: b.3,
            middle_name: b.4,
            birth_date: c.0,
            gender: c.1,
            registration_address: c.2,
            comment: d.0,
            contacts: d.1,
            role: d.2,
            deletion_mark: d.3,
            version: d.4,
            bank_accounts: accounts,
            ..Counterparty::default()
        })
        .boxed()
}

fn counterparty() -> BoxedStrategy<Counterparty> {
    (
        simple_counterparty(),
        vec((text(), simple_counterparty()), 0..2),
        of(simple_counterparty()),
        of(warehouse()),
        of(bank_account()),
    )
        .prop_map(|(mut c, reps, org, warehouse, account)| {
            c.representatives = reps
                .into_iter()
                .map(|(relation, counterparty)| Representative {
                    relation,
                    counterparty,
                })
                .collect();
            c.organization = org.map(Box::new);
            c.warehouse = warehouse;
            c.bank_account = account;
            c
        })
        .boxed()
}

fn property() -> BoxedStrategy<Property> {
    (
        (text(), text(), opt_text(), opt_bool()),
        of(prop_oneof![
            Just(PropertyValueType::Dictionary),
            Just(PropertyValueType::Number),
            other().prop_map(PropertyValueType::Other)
        ]),
        of(prop_oneof![
            Just(RequiredFor::Catalog),
            other().prop_map(RequiredFor::Other)
        ]),
        vec(text(), 0..2),
        vec(
            (opt_text(), text(), opt_text()).prop_map(|(value_id, value, picture)| DictionaryValue {
                value_id,
                value,
                picture,
            }),
            0..3,
        ),
        (opt_bool(), opt_bool(), opt_bool(), opt_bool()),
    )
        .prop_map(
            |(a, value_type, required_for, values, dictionary, flags)| Property {
                id: a.0,
                name: a.1,
                description: a.2,
                multiple: a.3,
                value_type,
                required_for,
                values,
                dictionary,
                for_products: flags.0,
                for_offers: flags.1,
                for_documents: flags.2,
                external: flags.3,
                ..Property::default()
            },
        )
        .boxed()
}

fn group() -> BoxedStrategy<Group> {
    let leaf =
        (text(), text(), opt_text(), opt_bool()).prop_map(|(id, name, description, deletion_mark)| Group {
            id,
            name,
            description,
            deletion_mark,
            ..Group::default()
        });
    leaf.prop_recursive(2, 6, 3, |inner| {
        (text(), text(), vec(inner, 0..3), vec(property(), 0..2)).prop_map(
            |(id, name, groups, properties)| Group {
                id,
                name,
                groups,
                properties,
                ..Group::default()
            },
        )
    })
    .boxed()
}

fn price_type() -> BoxedStrategy<PriceType> {
    (
        text(),
        opt_text(),
        opt_text(),
        vec(
            (text(), opt_bool(), opt_bool()).prop_map(|(name, included_in_amount, excise)| Tax {
                name,
                included_in_amount,
                excise,
            }),
            0..2,
        ),
    )
        .prop_map(|(id, name, currency, taxes)| PriceType {
            id,
            name,
            currency,
            taxes,
            ..PriceType::default()
        })
        .boxed()
}

fn classifier() -> BoxedStrategy<Classifier> {
    (
        (text(), text(), of(simple_counterparty()), opt_text()),
        vec(group(), 0..3),
        vec(property(), 0..3),
        vec(price_type(), 0..2),
        vec(warehouse(), 0..2),
        vec(
            (opt_text(), opt_text(), opt_text(), opt_text(), opt_text()).prop_map(|u| Unit {
                id: u.0,
                code: u.1,
                full_name: u.2,
                short_name: u.3,
                international_code: u.4,
                ..Unit::default()
            }),
            0..2,
        ),
        vec(
            (text(), text(), opt_text()).prop_map(|(id, name, description)| Category {
                id,
                name,
                description,
                ..Category::default()
            }),
            0..2,
        ),
    )
        .prop_map(
            |(h, groups, properties, price_types, warehouses, units, categories)| Classifier {
                id: h.0,
                name: h.1,
                owner: h.2,
                description: h.3,
                groups,
                properties,
                price_types,
                warehouses,
                units,
                categories,
                ..Classifier::default()
            },
        )
        .boxed()
}

fn unit_conversion() -> BoxedStrategy<UnitConversion> {
    (opt_text(), opt_text(), vec(requisite(), 0..2))
        .prop_map(|(unit, coefficient, extra_data)| UnitConversion {
            unit,
            coefficient,
            extra_data,
        })
        .boxed()
}

fn base_unit() -> BoxedStrategy<BaseUnit> {
    (
        opt_text(),
        opt_text(),
        opt_text(),
        opt_text(),
        of(word()),
        vec(unit_conversion(), 0..2),
    )
        .prop_map(
            |(code, full_name, international_code, short_name, text, conversions)| BaseUnit {
                code,
                full_name,
                international_code,
                short_name,
                text,
                conversions,
            },
        )
        .boxed()
}

/// A product without components or analogs.
fn simple_product() -> BoxedStrategy<Product> {
    (
        (
            text(),
            opt_text(),
            opt_text(),
            text(),
            of(base_unit()),
            opt_text(),
        ),
        (
            vec(text(), 0..3),
            opt_text(),
            opt_text(),
            vec(text(), 0..2),
            opt_text(),
            opt_text(),
        ),
        vec(
            (text(), opt_text(), vec(text(), 0..2)).prop_map(|(id, name, values)| PropertyValue {
                id,
                name,
                values,
            }),
            0..3,
        ),
        vec(
            (text(), text()).prop_map(|(name, rate)| TaxRate { name, rate }),
            0..2,
        ),
        vec(
            (text(), opt_decimal(), text()).prop_map(|(name, amount_per_unit, currency)| Excise {
                name,
                amount_per_unit,
                currency,
            }),
            0..2,
        ),
        vec(
            (opt_text(), text(), text()).prop_map(|(id, name, value)| Characteristic { id, name, value }),
            0..2,
        ),
        vec(requisite(), 0..3),
        (
            of(prop_oneof![
                Just(ProductStatus::New),
                Just(ProductStatus::Deleted),
                other().prop_map(ProductStatus::Other)
            ]),
            opt_bool(),
            opt_text(),
            opt_decimal(),
            of(simple_counterparty()),
        ),
    )
        .prop_map(
            |(a, b, property_values, tax_rates, excises, characteristics, requisites, c)| Product {
                id: a.0,
                barcode: a.1,
                article: a.2,
                name: a.3,
                base_unit: a.4,
                counterparty_product_id: a.5,
                group_ids: b.0,
                category_id: b.1,
                description: b.2,
                pictures: b.3,
                country: b.4,
                trademark: b.5,
                property_values,
                tax_rates,
                excises,
                characteristics,
                requisites,
                status: c.0,
                deletion_mark: c.1,
                version: c.2,
                weight: c.3,
                manufacturer: c.4.map(Box::new),
                ..Product::default()
            },
        )
        .boxed()
}

fn product() -> BoxedStrategy<Product> {
    (
        simple_product(),
        vec(
            (simple_product(), opt_text(), opt_text(), opt_decimal()).prop_map(
                |(p, catalog_id, classifier_id, quantity)| RelatedProduct {
                    product: Box::new(p),
                    catalog_id,
                    classifier_id,
                    quantity,
                },
            ),
            0..2,
        ),
    )
        .prop_map(|(mut p, related)| {
            p.analogs = related
                .iter()
                .cloned()
                .map(|mut r| {
                    r.quantity = None;
                    r
                })
                .collect();
            p.components = related;
            p
        })
        .boxed()
}

fn price() -> BoxedStrategy<Price> {
    (
        opt_text(),
        text(),
        opt_decimal(),
        opt_text(),
        unit_conversion(),
        opt_decimal(),
        opt_text(),
    )
        .prop_map(
            |(presentation, price_type_id, price_per_unit, currency, unit, min_quantity, catalog_id)| Price {
                presentation,
                price_type_id,
                price_per_unit,
                currency,
                unit,
                min_quantity,
                catalog_id,
            },
        )
        .boxed()
}

fn stock() -> BoxedStrategy<WarehouseStock> {
    (text(), opt_decimal())
        .prop_map(|(warehouse_id, quantity)| WarehouseStock {
            warehouse_id,
            quantity,
        })
        .boxed()
}

fn offer() -> BoxedStrategy<Offer> {
    (
        simple_product(),
        vec(price(), 0..3),
        opt_decimal(),
        vec(stock(), 0..3),
        vec(
            (opt_decimal(), of(stock())).prop_map(|(quantity, warehouse)| Rest { quantity, warehouse }),
            0..2,
        ),
    )
        .prop_map(|(product, prices, quantity, warehouse_stock, rests)| Offer {
            product,
            prices,
            quantity,
            warehouse_stock,
            rests,
        })
        .boxed()
}

fn offer_package() -> BoxedStrategy<OfferPackage> {
    (
        (opt_text(), text(), text(), opt_text(), opt_text(), opt_text()),
        (opt_text(), of(simple_counterparty()), opt_text(), opt_bool()),
        vec(price_type(), 0..2),
        vec(warehouse(), 0..2),
        vec(
            (text(), opt_text(), vec(text(), 0..2)).prop_map(|(id, name, values)| PropertyValue {
                id,
                name,
                values,
            }),
            0..2,
        ),
        vec(offer(), 0..4),
    )
        .prop_map(
            |(a, b, price_types, warehouses, property_values, offers)| OfferPackage {
                id: a.0,
                name: a.1,
                catalog_id: a.2,
                classifier_id: a.3,
                valid_from: a.4,
                valid_to: a.5,
                description: b.0,
                owner: b.1,
                price_types_classifier_id: b.2,
                changes_only: b.3,
                price_types,
                warehouses,
                property_values,
                offers,
                ..OfferPackage::default()
            },
        )
        .boxed()
}

fn adjustment() -> BoxedStrategy<Adjustment> {
    (text(), opt_decimal(), opt_text(), opt_bool(), opt_text())
        .prop_map(
            |(name, amount, percent, included_in_amount, comment)| Adjustment {
                name,
                amount,
                percent,
                included_in_amount,
                comment,
            },
        )
        .boxed()
}

fn document_tax() -> BoxedStrategy<DocumentTax> {
    (text(), opt_bool(), opt_decimal(), opt_text())
        .prop_map(|(name, included, amount, rate)| DocumentTax {
            tax: Tax {
                name,
                included_in_amount: included,
                excise: None,
            },
            amount,
            rate,
        })
        .boxed()
}

fn document_item() -> BoxedStrategy<DocumentItem> {
    (
        simple_product(),
        (
            opt_text(),
            opt_text(),
            opt_decimal(),
            opt_decimal(),
            opt_decimal(),
            unit_conversion(),
        ),
        (
            opt_text(),
            opt_text(),
            vec(document_tax(), 0..2),
            vec(adjustment(), 0..2),
            vec(adjustment(), 0..2),
        ),
        vec(requisite(), 0..2),
        vec(
            (warehouse(), opt_decimal())
                .prop_map(|(warehouse, quantity)| WarehouseQuantity { warehouse, quantity }),
            0..2,
        ),
    )
        .prop_map(
            |(product, a, b, additional_requisites, warehouses)| DocumentItem {
                product,
                catalog_id: a.0,
                classifier_id: a.1,
                price_per_unit: a.2,
                quantity: a.3,
                amount: a.4,
                unit: a.5,
                country_of_origin: b.0,
                customs_declaration: b.1,
                taxes: b.2,
                discounts: b.3,
                extra_charges: b.4,
                additional_requisites,
                warehouses,
            },
        )
        .boxed()
}

fn operation() -> BoxedStrategy<BusinessOperation> {
    prop_oneof![
        Just(BusinessOperation::OrderGoods),
        Just(BusinessOperation::ReleaseGoods),
        other().prop_map(BusinessOperation::Other),
    ]
    .boxed()
}

fn simple_document() -> BoxedStrategy<Document> {
    (
        (
            text(),
            text(),
            text(),
            of(operation()),
            of(role()),
            opt_text(),
            opt_text(),
            opt_decimal(),
        ),
        vec(counterparty(), 0..2),
        (opt_text(), opt_text(), opt_text()),
        (
            vec(document_tax(), 0..2),
            vec(adjustment(), 0..2),
            vec(adjustment(), 0..2),
            vec(warehouse(), 0..2),
        ),
        vec(document_item(), 0..3),
        vec(requisite(), 0..3),
        (opt_text(), opt_text(), opt_text(), opt_bool(), opt_text()),
        vec(
            (text(), text(), text(), opt_decimal(), text(), opt_text()).prop_map(|p| SitePayment {
                document_number: p.0,
                transaction_number: p.1,
                date: p.2,
                amount: p.3,
                method: p.4,
                method_id: p.5,
            }),
            0..2,
        ),
    )
        .prop_map(
            |(a, counterparties, b, c, items, requisites, d, payments)| Document {
                id: a.0,
                number: a.1,
                date: a.2,
                operation: a.3,
                role: a.4,
                currency: a.5,
                rate: a.6,
                amount: a.7,
                counterparties,
                time: b.0,
                payment_due: b.1,
                comment: b.2,
                taxes: c.0,
                discounts: c.1,
                extra_charges: c.2,
                warehouses: c.3,
                items,
                requisites,
                number_1c: d.0,
                date_1c: d.1,
                basis: d.2,
                deletion_mark: d.3,
                version: d.4,
                payments,
                ..Document::default()
            },
        )
        .boxed()
}

fn document() -> BoxedStrategy<Document> {
    (simple_document(), vec(simple_document(), 0..2))
        .prop_map(|(mut d, subs)| {
            d.subordinate_documents = subs;
            d
        })
        .boxed()
}

fn changes() -> BoxedStrategy<OfferPackageChanges> {
    (
        text(),
        opt_text(),
        opt_bool(),
        vec(
            (
                (opt_text(), opt_text(), opt_text(), opt_text(), opt_text()),
                vec(stock(), 0..2),
                vec(price(), 0..2),
                opt_decimal(),
            )
                .prop_map(|(a, warehouse_stock, prices, quantity)| OfferChange {
                    id: a.0,
                    barcode: a.1,
                    article: a.2,
                    characteristic_id: a.3,
                    unit_code: a.4,
                    warehouse_stock,
                    prices,
                    quantity,
                    extra: Vec::new(),
                }),
            0..3,
        ),
    )
        .prop_map(|(id, catalog_id, changes_only, offers)| OfferPackageChanges {
            id,
            catalog_id,
            changes_only,
            offers,
            extra: Vec::new(),
        })
        .boxed()
}

fn info() -> BoxedStrategy<CommercialInfo> {
    (
        (text(), text()),
        of(classifier()),
        of((
            text(),
            opt_text(),
            text(),
            of(simple_counterparty()),
            vec(product(), 0..4),
            opt_text(),
            opt_bool(),
        )),
        of(offer_package()),
        vec(changes(), 0..2),
        vec(document(), 0..3),
        vec(counterparty(), 0..2),
    )
        .prop_map(
            |(h, classifier, catalog, offer_package, offer_package_changes, documents, counterparties)| {
                CommercialInfo {
                    schema_version: h.0,
                    generated_at: h.1,
                    classifier,
                    catalog: catalog.map(|c| Catalog {
                        id: c.0,
                        classifier_id: c.1,
                        name: c.2,
                        owner: c.3,
                        products: c.4,
                        description: c.5,
                        changes_only: c.6,
                        ..Catalog::default()
                    }),
                    offer_package,
                    offer_package_changes,
                    documents,
                    counterparties,
                    extra: Vec::new(),
                }
            },
        )
        .boxed()
}

/// `PROPTEST_CASES` overrides the default number of cases.
fn cases_or(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Runs a property over `info()` in a thread with a large stack: the
/// strategy tree is deep and debug builds use big stack frames.
fn check_info(cases: u32, property: fn(&CommercialInfo) -> Result<(), TestCaseError>) {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let cases = cases_or(cases);
            let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig {
                cases,
                ..ProptestConfig::default()
            });
            runner.run(&info(), |v| property(&v)).unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn write_then_parse_is_identity() {
    check_info(128, |info| {
        let xml = to_string(info).unwrap();
        let parsed = from_str(&xml).unwrap();
        prop_assert!(
            parsed.warnings.is_empty(),
            "{:?}
{xml}",
            parsed.warnings
        );
        prop_assert_eq!(&parsed.info, info);
        Ok(())
    });
}

#[test]
fn compact_and_windows_1251_output_round_trip() {
    check_info(64, |info| {
        let options = WriteOptions::default()
            .indent(false)
            .encoding(Encoding::WINDOWS_1251);
        let bytes = options.to_vec(info).unwrap();
        let parsed = from_slice(&bytes).unwrap();
        prop_assert_eq!(&parsed.info, info);
        Ok(())
    });
}

const FIXTURE_TEXTS: [&str; 6] = [
    include_str!("fixtures/import_2_10.xml"),
    include_str!("fixtures/offers_2_07.xml"),
    include_str!("fixtures/rests_2_09.xml"),
    include_str!("fixtures/orders_2_03.xml"),
    include_str!("fixtures/documents_2_09_2_10.xml"),
    include_str!("fixtures/misc_extensions.xml"),
];

proptest! {
    #![proptest_config(ProptestConfig { cases: cases_or(256), ..ProptestConfig::default() })]

    #[test]
    fn decimal_display_parse(m in any::<i64>(), s in 0u32..=18) {
        let d = Decimal::new(i128::from(m), s);
        let back: Decimal = d.to_string().parse().unwrap();
        prop_assert_eq!(back, d);
        prop_assert_eq!(back.scale(), s);
    }

    #[test]
    fn parser_never_panics(bytes in vec(any::<u8>(), 0..512)) {
        let _ = from_slice(&bytes);
    }

    /// The invariant of the `roundtrip` fuzz target, on damaged fixtures:
    /// whatever parses can be written, re-parsed, and written identically.
    #[test]
    fn mutated_xml_never_panics_and_writing_is_a_fixed_point(
        fixture in 0usize..6,
        cut in 0usize..6000,
        junk in "[<>&/;#xa-zА-Я0-9\"'= \u{1}\u{FFFE}\r]{0,6}",
    ) {
        let xml = FIXTURE_TEXTS[fixture];
        let cut = cut.min(xml.len());
        let cut = (0..=cut).rev().find(|&i| xml.is_char_boundary(i)).unwrap_or(0);
        let mutated = format!("{}{junk}{}", &xml[..cut], &xml[cut..]);
        if let Ok(first) = from_str(&mutated) {
            let out1 = to_string(&first.info).unwrap();
            let second = from_str(&out1).unwrap();
            let out2 = to_string(&second.info).unwrap();
            prop_assert_eq!(out1, out2);
        }
    }
}
