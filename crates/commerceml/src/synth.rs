//! Deterministic generator of synthetic CommerceML exports.
//!
//! All names, ids and numbers are made up; the same [`Config`] always
//! produces the same bytes. Used by the benchmarks, the tests and the CLI
//! (`commerceml generate`) to get realistic files of any size without real
//! company data.
//!
//! ```
//! use commerceml::synth::Config;
//!
//! let cfg = Config { products: 50, ..Config::default() };
//! let xml = cfg.write_import(Vec::new(), &Default::default())?;
//! let parsed = commerceml::from_slice(&xml)?;
//! assert_eq!(parsed.info.products().len(), 50);
//! # Ok::<(), commerceml::Error>(())
//! ```

use std::io::Write;

use crate::Decimal;
use crate::error::Result;
use crate::model::{
    Address, AddressField, AddressFieldType, BaseUnit, Catalog, Characteristic, Classifier, CommercialInfo,
    Contact, ContactType, Counterparty, DictionaryValue, Document, DocumentItem, Group, Offer, OfferPackage,
    Price, PriceType, Product, Property, PropertyValue, PropertyValueType, Representative, Requisite, Rest,
    Role, Tax, TaxRate, UnitConversion, Warehouse, WarehouseStock,
};
use crate::stream::Header;
use crate::write::{WriteOptions, Writer};

/// What to generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Number of products (and offers).
    pub products: usize,
    /// Number of top-level groups; each gets three subgroups.
    pub groups: usize,
    /// Number of properties; every fourth is a dictionary property.
    pub properties: usize,
    /// Number of price types.
    pub price_types: usize,
    /// Number of warehouses.
    pub warehouses: usize,
    /// Every n-th product gets three variants (`product#characteristic`
    /// offers). Zero disables variants.
    pub variants_every: usize,
    /// Seed of the pseudo-random generator.
    pub seed: u64,
    /// `ДатаФормирования` of generated files.
    pub generated_at: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            products: 1000,
            groups: 10,
            properties: 12,
            price_types: 2,
            warehouses: 3,
            variants_every: 10,
            seed: 42,
            generated_at: "2026-11-02T12:00:00".to_owned(),
        }
    }
}

/// SplitMix64: tiny, fast and good enough for test data.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// A GUID-shaped id derived from a kind tag and an index.
fn guid(kind: u64, index: usize) -> String {
    let mut r = Rng::new(kind.wrapping_mul(0x1000_0000_01B3) ^ index as u64);
    let (a, b) = (r.next(), r.next());
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0xFFFF,
        b >> 48,
        b & 0xFFFF_FFFF_FFFF
    )
}

const NOUNS: &[&str] = &[
    "Стол",
    "Стул",
    "Лампа",
    "Полка",
    "Кресло",
    "Шкаф",
    "Ковёр",
    "Зеркало",
    "Чайник",
    "Кружка",
    "Тарелка",
    "Подушка",
    "Плед",
    "Корзина",
    "Вешалка",
    "Табурет",
    "Часы",
    "Ваза",
    "Рамка",
    "Коврик",
];
const ADJECTIVES: &[&str] = &[
    "складной",
    "дубовый",
    "компактный",
    "угловой",
    "детский",
    "садовый",
    "дорожный",
    "белый",
    "серый",
    "синтетический",
    "классический",
    "модульный",
];
const COLORS: &[&str] = &["Красный", "Синий", "Зелёный", "Белый", "Чёрный", "Серый"];
const SIZES: &[&str] = &["S", "M", "L", "XL"];
const UNITS: &[(&str, &str, &str, &str)] = &[
    ("796", "Штука", "PCE", "шт"),
    ("166", "Килограмм", "KGM", "кг"),
    ("006", "Метр", "MTR", "м"),
    ("704", "Набор", "SET", "набор"),
];

fn amount(r: &mut Rng, max_rubles: u64) -> Decimal {
    let kopecks = 100 + r.next() % (max_rubles * 100);
    Decimal::new(i128::from(kopecks), 2)
}

impl Config {
    fn header(&self) -> Header {
        Header {
            schema_version: crate::model::DEFAULT_SCHEMA_VERSION.to_owned(),
            generated_at: self.generated_at.clone(),
        }
    }

    fn owner() -> Counterparty {
        Counterparty {
            id: Some(guid(1, 0)),
            name: Some("Синтетический поставщик".into()),
            official_name: Some("ООО «Синтетический поставщик»".into()),
            inn: Some("0000000000".into()),
            kpp: Some("000000000".into()),
            ..Counterparty::default()
        }
    }

    fn group_id(i: usize) -> String {
        guid(2, i)
    }

    fn property_id(i: usize) -> String {
        guid(3, i)
    }

    fn price_type_id(i: usize) -> String {
        guid(4, i)
    }

    fn warehouse_id(i: usize) -> String {
        guid(5, i)
    }

    /// The id of product `i`.
    pub fn product_id(&self, i: usize) -> String {
        guid(6 ^ self.seed, i)
    }

    /// The classifier: groups, properties, price types, warehouses.
    pub fn classifier(&self) -> Classifier {
        let groups = (0..self.groups)
            .map(|g| Group {
                id: Self::group_id(g),
                name: format!("Раздел {}", g + 1),
                groups: (0..3)
                    .map(|s| Group {
                        id: Self::group_id(1000 + g * 3 + s),
                        name: format!("Подраздел {}.{}", g + 1, s + 1),
                        ..Group::default()
                    })
                    .collect(),
                ..Group::default()
            })
            .collect();
        let properties = (0..self.properties)
            .map(|p| {
                let dictionary = p % 4 == 0;
                Property {
                    id: Self::property_id(p),
                    name: if dictionary {
                        format!("Цвет {p}")
                    } else {
                        format!("Характеристика {p}")
                    },
                    value_type: Some(if dictionary {
                        PropertyValueType::Dictionary
                    } else {
                        PropertyValueType::String
                    }),
                    dictionary: if dictionary {
                        COLORS
                            .iter()
                            .enumerate()
                            .map(|(i, c)| DictionaryValue {
                                value_id: Some(guid(7, p * 100 + i)),
                                value: (*c).to_owned(),
                                picture: None,
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    for_products: Some(true),
                    ..Property::default()
                }
            })
            .collect();
        Classifier {
            id: guid(8, 0),
            name: "Классификатор (синтетический)".into(),
            owner: Some(Self::owner()),
            groups,
            properties,
            price_types: self.price_types(),
            warehouses: self.warehouses(),
            ..Classifier::default()
        }
    }

    fn price_types(&self) -> Vec<PriceType> {
        (0..self.price_types)
            .map(|t| PriceType {
                id: Self::price_type_id(t),
                name: Some(if t == 0 {
                    "Розничная".into()
                } else {
                    format!("Оптовая {t}")
                }),
                currency: Some("RUB".into()),
                taxes: vec![Tax {
                    name: "НДС".into(),
                    included_in_amount: Some(true),
                    excise: Some(false),
                }],
                ..PriceType::default()
            })
            .collect()
    }

    fn warehouses(&self) -> Vec<Warehouse> {
        (0..self.warehouses)
            .map(|w| Warehouse {
                id: Self::warehouse_id(w),
                name: Some(format!("Склад №{}", w + 1)),
                ..Warehouse::default()
            })
            .collect()
    }

    /// Product number `i`.
    pub fn product(&self, i: usize) -> Product {
        let mut r = Rng::new(self.seed ^ (i as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
        let (code, full, intl, short) = UNITS[r.below(UNITS.len())];
        let name = format!("{} {} {}", r.pick(NOUNS), r.pick(ADJECTIVES), i + 1);
        let group = if self.groups == 0 {
            Vec::new()
        } else {
            vec![Self::group_id(1000 + r.below(self.groups * 3))]
        };
        let mut description = format!("Описание товара «{name}». Размеры: 30×40 см.");
        if i.is_multiple_of(7) {
            description.push_str(" Состав: дерево & металл <без примесей>.");
        }
        let property_values = (0..self.properties.min(4))
            .map(|p| {
                let pid = (i + p) % self.properties.max(1);
                let value = if pid.is_multiple_of(4) {
                    guid(7, pid * 100 + r.below(COLORS.len()))
                } else {
                    format!("{}", r.below(100))
                };
                PropertyValue {
                    id: Self::property_id(pid),
                    name: None,
                    values: vec![value],
                }
            })
            .collect();
        Product {
            id: self.product_id(i),
            barcode: i.is_multiple_of(3).then(|| format!("46{i:011}")),
            article: Some(format!("ART-{:06}", i + 1)),
            name: name.clone(),
            base_unit: Some(BaseUnit {
                code: Some(code.into()),
                full_name: Some(full.into()),
                international_code: Some(intl.into()),
                short_name: None,
                text: Some(short.into()),
                conversions: vec![UnitConversion {
                    unit: Some(code.into()),
                    coefficient: Some("1".into()),
                    extra_data: Vec::new(),
                }],
            }),
            group_ids: group,
            description: Some(description),
            pictures: i
                .is_multiple_of(2)
                .then(|| format!("import_files/{:02x}/{}.jpg", i % 256, self.product_id(i)))
                .into_iter()
                .collect(),
            country: Some("РОССИЯ".into()),
            property_values,
            tax_rates: vec![TaxRate {
                name: "НДС".into(),
                rate: "20".into(),
            }],
            requisites: vec![
                Requisite::new("ВидНоменклатуры", "Товар"),
                Requisite::new("ТипНоменклатуры", "Запас"),
                Requisite::new("Полное наименование", name),
            ],
            ..Product::default()
        }
    }

    fn offer_for(&self, i: usize, variant: Option<usize>) -> Offer {
        let mut r = Rng::new(self.seed ^ 0xABCD ^ (i as u64) << 8 ^ variant.unwrap_or(9) as u64);
        let base = self.product(i);
        let (id, name, characteristics) = match variant {
            Some(v) => (
                format!("{}#{}", base.id, guid(9, i * 10 + v)),
                format!("{} ({})", base.name, SIZES[v % SIZES.len()]),
                vec![Characteristic {
                    id: None,
                    name: "Размер".into(),
                    value: SIZES[v % SIZES.len()].into(),
                }],
            ),
            None => (base.id.clone(), base.name.clone(), Vec::new()),
        };
        let retail = amount(&mut r, 50_000);
        let prices = (0..self.price_types)
            .map(|t| {
                let price = if t == 0 {
                    retail
                } else {
                    let factor = Decimal::new(100 - 10 * i128::try_from(t.min(5)).unwrap_or(0), 2);
                    retail
                        .checked_mul(factor)
                        .map_or(retail, |p| Decimal::new(p.mantissa() / 100, 2))
                };
                Price {
                    presentation: Some(format!(
                        "{price} RUB за {}",
                        base.base_unit
                            .as_ref()
                            .and_then(|u| u.text.as_deref())
                            .unwrap_or("шт")
                    )),
                    price_type_id: Self::price_type_id(t),
                    price_per_unit: Some(price),
                    currency: Some("RUB".into()),
                    unit: UnitConversion {
                        unit: base.base_unit.as_ref().and_then(|u| u.code.clone()),
                        coefficient: Some("1".into()),
                        extra_data: Vec::new(),
                    },
                    ..Price::default()
                }
            })
            .collect();
        let stock: Vec<WarehouseStock> = (0..self.warehouses)
            .map(|w| WarehouseStock {
                warehouse_id: Self::warehouse_id(w),
                quantity: Some(Decimal::from(u32::try_from(r.below(200)).unwrap_or(0))),
            })
            .collect();
        let total = stock
            .iter()
            .filter_map(|s| s.quantity)
            .try_fold(Decimal::ZERO, Decimal::checked_add);
        let rests = if i.is_multiple_of(5) {
            stock
                .iter()
                .map(|s| Rest {
                    quantity: None,
                    warehouse: Some(s.clone()),
                })
                .collect()
        } else {
            Vec::new()
        };
        Offer {
            product: Product {
                id,
                article: base.article.clone(),
                name,
                base_unit: base.base_unit.clone(),
                characteristics,
                ..Product::default()
            },
            prices,
            quantity: total,
            warehouse_stock: stock,
            rests,
        }
    }

    /// Offers for product `i`: one, or three variants.
    pub fn offers_for(&self, i: usize) -> Vec<Offer> {
        if self.variants_every > 0 && i.is_multiple_of(self.variants_every) {
            (0..3).map(|v| self.offer_for(i, Some(v))).collect()
        } else {
            vec![self.offer_for(i, None)]
        }
    }

    /// Order number `i` with a few lines from the catalog.
    pub fn order(&self, i: usize) -> Document {
        let mut r = Rng::new(self.seed ^ 0x0DE0 ^ i as u64);
        let mut order = Document::order(
            format!("site-order-{}", i + 1),
            format!("{}", 1000 + i),
            format!("2026-{:02}-{:02}", 1 + r.below(12), 1 + r.below(28)),
        );
        order.currency = Some("RUB".into());
        order.time = Some(format!("{:02}:{:02}:00", r.below(24), r.below(60)));
        order.comment = Some("Позвонить за час & не звонить в домофон".into());
        order.counterparties.push(Counterparty {
            id: Some(format!("customer-{}", i % 97)),
            name: Some(format!("Покупатель {}", i % 97)),
            full_name: Some(format!("Тестов Тест Тестович {}", i % 97)),
            last_name: Some("Тестов".into()),
            first_name: Some("Тест".into()),
            role: Some(Role::Buyer),
            registration_address: Some(Address {
                presentation: "000000, г. Примерный, ул. Тестовая, д. 1".into(),
                comment: None,
                fields: vec![
                    AddressField {
                        kind: AddressFieldType::PostalCode,
                        value: "000000".into(),
                    },
                    AddressField {
                        kind: AddressFieldType::City,
                        value: "Примерный".into(),
                    },
                ],
            }),
            contacts: vec![Contact {
                kind: ContactType::Email,
                value: format!("customer{}@example.invalid", i % 97),
                comment: None,
            }],
            representatives: vec![Representative {
                relation: "Контактное лицо".into(),
                counterparty: Counterparty {
                    id: Some(format!("contact-{}", i % 97)),
                    name: Some("Тест Тестов".into()),
                    ..Counterparty::default()
                },
            }],
            ..Counterparty::default()
        });
        for line in 0..=r.below(4) {
            let p = self.product((i * 7 + line * 13) % self.products.max(1));
            let qty = Decimal::from(u32::try_from(1 + r.below(5)).unwrap_or(1));
            let price = amount(&mut r, 5_000);
            order.items.push(DocumentItem {
                product: Product {
                    id: p.id,
                    name: p.name,
                    base_unit: p.base_unit,
                    requisites: vec![Requisite::new("ВидНоменклатуры", "Товар")],
                    ..Product::default()
                },
                price_per_unit: Some(price),
                quantity: Some(qty),
                amount: price.checked_mul(qty),
                ..DocumentItem::default()
            });
        }
        order.amount = order.items_total();
        order.requisites = vec![
            Requisite::new("Метод оплаты", "Банковская карта"),
            Requisite::new(
                "Заказ оплачен",
                if i.is_multiple_of(2) { "true" } else { "false" },
            ),
            Requisite::new("Статус заказа", "[N] Принят"),
        ];
        order
    }

    /// Writes `import.xml`: classifier and catalog.
    pub fn write_import<W: Write>(&self, out: W, options: &WriteOptions) -> Result<W> {
        let mut w = Writer::new(out, &self.header(), options)?;
        w.classifier(&self.classifier())?;
        w.begin_catalog(&Catalog {
            id: guid(10, 0),
            classifier_id: Some(guid(8, 0)),
            name: "Основной каталог (синтетический)".into(),
            owner: Some(Self::owner()),
            changes_only: Some(false),
            ..Catalog::default()
        })?;
        for i in 0..self.products {
            w.product(&self.product(i))?;
        }
        w.end_catalog()?;
        w.finish()
    }

    /// Writes `offers.xml`: an offer package with prices and stock.
    pub fn write_offers<W: Write>(&self, out: W, options: &WriteOptions) -> Result<W> {
        let mut w = Writer::new(out, &self.header(), options)?;
        w.begin_offer_package(&OfferPackage {
            id: Some(format!("{}#", guid(10, 0))),
            name: "Пакет предложений (синтетический)".into(),
            catalog_id: guid(10, 0),
            classifier_id: Some(guid(8, 0)),
            owner: Some(Self::owner()),
            price_types: self.price_types(),
            warehouses: self.warehouses(),
            changes_only: Some(false),
            ..OfferPackage::default()
        })?;
        for i in 0..self.products {
            for o in self.offers_for(i) {
                w.offer(&o)?;
            }
        }
        w.end_offer_package()?;
        w.finish()
    }

    /// Writes a file with `count` orders.
    pub fn write_orders<W: Write>(&self, count: usize, out: W, options: &WriteOptions) -> Result<W> {
        let mut w = Writer::new(out, &self.header(), options)?;
        for i in 0..count {
            w.document(&self.order(i))?;
        }
        w.finish()
    }

    /// Everything in memory: classifier, catalog, offers and `orders` orders.
    pub fn commercial_info(&self, orders: usize) -> CommercialInfo {
        let mut info = CommercialInfo::new(self.generated_at.clone());
        info.classifier = Some(self.classifier());
        info.catalog = Some(Catalog {
            id: guid(10, 0),
            name: "Основной каталог (синтетический)".into(),
            products: (0..self.products).map(|i| self.product(i)).collect(),
            ..Catalog::default()
        });
        info.offer_package = Some(OfferPackage {
            name: "Пакет предложений (синтетический)".into(),
            catalog_id: guid(10, 0),
            price_types: self.price_types(),
            warehouses: self.warehouses(),
            offers: (0..self.products).flat_map(|i| self.offers_for(i)).collect(),
            ..OfferPackage::default()
        });
        info.documents = (0..orders).map(|i| self.order(i)).collect();
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_parseable() {
        let cfg = Config {
            products: 30,
            ..Config::default()
        };
        let a = cfg.write_offers(Vec::new(), &WriteOptions::default()).unwrap();
        let b = cfg.write_offers(Vec::new(), &WriteOptions::default()).unwrap();
        assert_eq!(a, b);
        let parsed = crate::from_slice(&a).unwrap();
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.info.offers().len(), 30 + 3 * 2);
        let orders = cfg.write_orders(5, Vec::new(), &WriteOptions::default()).unwrap();
        let parsed = crate::from_slice(&orders).unwrap();
        assert_eq!(parsed.info.documents.len(), 5);
        let info = cfg.commercial_info(2);
        assert!(
            crate::validate::validate(&info).is_empty(),
            "{:?}",
            crate::validate::validate(&info)
        );
    }

    #[test]
    fn guid_shape() {
        let g = guid(1, 2);
        assert_eq!(g.len(), 36);
        assert_eq!(g.matches('-').count(), 4);
        assert_ne!(guid(1, 2), guid(1, 3));
    }
}
