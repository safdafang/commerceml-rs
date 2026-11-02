use super::catalog::{Product, PropertyValue, UnitConversion};
use super::classifier::PriceType;
use super::common::{RawElement, Warehouse};
use super::counterparty::{Counterparty, Signatory};
use crate::Decimal;

/// An offer package: `ПакетПредложений` — prices and stock (a price list).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OfferPackage {
    /// `Ид`.
    pub id: Option<String>,
    /// `Наименование`.
    pub name: String,
    /// `ИдКаталога`.
    pub catalog_id: String,
    /// `ИдКлассификатора`.
    pub classifier_id: Option<String>,
    /// `ДействительноС`.
    pub valid_from: Option<String>,
    /// `ДействительноДо`.
    pub valid_to: Option<String>,
    /// `Описание`.
    pub description: Option<String>,
    /// `Владелец`.
    pub owner: Option<Counterparty>,
    /// `ТипыЦен/ИдКлассификатора`.
    pub price_types_classifier_id: Option<String>,
    /// `ТипыЦен/ТипЦены`.
    pub price_types: Vec<PriceType>,
    /// `Склады/Склад`.
    pub warehouses: Vec<Warehouse>,
    /// `ЗначенияСвойств/ЗначенияСвойства` — common property values.
    pub property_values: Vec<PropertyValue>,
    /// `Предложения/Предложение`.
    pub offers: Vec<Offer>,
    /// `Подписанты/Подписант`.
    pub signatories: Vec<Signatory>,
    /// Attribute `СодержитТолькоИзменения`.
    pub changes_only: Option<bool>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// An offer: `Предложение` — a product (or product variant) with prices
/// and stock.
///
/// The standard defines `Предложение` as an extension of `Товар`, so all
/// product fields are available in [`product`](Self::product).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Offer {
    /// The product fields (`Ид`, `Наименование`, `ХарактеристикиТовара`…).
    pub product: Product,
    /// `Цены/Цена`.
    pub prices: Vec<Price>,
    /// `Количество` — total stock.
    pub quantity: Option<Decimal>,
    /// `Склад` with attributes `ИдСклада` and `КоличествоНаСкладе`.
    pub warehouse_stock: Vec<WarehouseStock>,
    /// `Остатки/Остаток` (de facto, `rests.xml`).
    pub rests: Vec<Rest>,
}

impl Offer {
    /// `Ид` of the offer.
    pub fn id(&self) -> &str {
        &self.product.id
    }

    /// The product part of `Ид` (before `#`).
    pub fn product_id(&self) -> &str {
        split_offer_id(&self.product.id).0
    }

    /// The characteristic part of `Ид` (after `#`), if any and not empty.
    pub fn characteristic_id(&self) -> Option<&str> {
        split_offer_id(&self.product.id).1
    }

    /// The price of the given price type.
    pub fn price(&self, price_type_id: &str) -> Option<&Price> {
        self.prices.iter().find(|p| p.price_type_id == price_type_id)
    }

    /// Total stock: `Количество` if present, otherwise the sum of
    /// [`rests`](Self::rests) or [`warehouse_stock`](Self::warehouse_stock).
    pub fn total_quantity(&self) -> Option<Decimal> {
        if let Some(q) = self.quantity {
            return Some(q);
        }
        let from_rests: Vec<Decimal> = self
            .rests
            .iter()
            .filter_map(|r| r.quantity.or(r.warehouse.as_ref().and_then(|w| w.quantity)))
            .collect();
        let parts = if from_rests.is_empty() {
            self.warehouse_stock.iter().filter_map(|w| w.quantity).collect()
        } else {
            from_rests
        };
        if parts.is_empty() {
            return None;
        }
        parts.into_iter().try_fold(Decimal::ZERO, Decimal::checked_add)
    }
}

/// Splits an offer id `product#characteristic` into its parts.
///
/// ```
/// assert_eq!(commerceml::split_offer_id("a#b"), ("a", Some("b")));
/// assert_eq!(commerceml::split_offer_id("a#"), ("a", None));
/// assert_eq!(commerceml::split_offer_id("a"), ("a", None));
/// ```
pub fn split_offer_id(id: &str) -> (&str, Option<&str>) {
    match id.split_once('#') {
        Some((p, c)) if !c.is_empty() => (p, Some(c)),
        Some((p, _)) => (p, None),
        None => (id, None),
    }
}

/// A price: `Цена`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Price {
    /// `Представление`, e.g. `1 499 руб. за шт`.
    pub presentation: Option<String>,
    /// `ИдТипаЦены`.
    pub price_type_id: String,
    /// `ЦенаЗаЕдиницу`.
    pub price_per_unit: Option<Decimal>,
    /// `Валюта`.
    pub currency: Option<String>,
    /// `Единица`, `Коэффициент` and `ДополнительныеДанные`.
    pub unit: UnitConversion,
    /// `МинКоличество`.
    pub min_quantity: Option<Decimal>,
    /// `ИдКаталога`.
    pub catalog_id: Option<String>,
}

/// Stock in one warehouse: `ОстаткиПоСкладам` (attributes `ИдСклада`,
/// `КоличествоНаСкладе`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WarehouseStock {
    /// `ИдСклада` (or `Ид` in `Остаток/Склад`).
    pub warehouse_id: String,
    /// `КоличествоНаСкладе` (or `Количество` in `Остаток/Склад`).
    pub quantity: Option<Decimal>,
}

/// A stock record: `Остаток` (de facto, `rests.xml`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rest {
    /// `Количество` — total over all exported warehouses.
    pub quantity: Option<Decimal>,
    /// `Склад` — stock in one warehouse.
    pub warehouse: Option<WarehouseStock>,
}

/// Changes to an offer package: `ИзмененияПакетаПредложений` (since 2.07).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OfferPackageChanges {
    /// `Ид`.
    pub id: String,
    /// `ИдКаталога`.
    pub catalog_id: Option<String>,
    /// `Предложения/Предложение`.
    pub offers: Vec<OfferChange>,
    /// Attribute `СодержитТолькоИзменения`.
    pub changes_only: Option<bool>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// A changed offer: `Предложение` inside `ИзмененияПакетаПредложений`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OfferChange {
    /// `Ид`.
    pub id: Option<String>,
    /// `Штрихкод`.
    pub barcode: Option<String>,
    /// `Артикул`.
    pub article: Option<String>,
    /// `ИдХарактеристики`.
    pub characteristic_id: Option<String>,
    /// `КодЕдиницыИзмерения`.
    pub unit_code: Option<String>,
    /// `Склады` with attributes `ИдСклада` and `КоличествоНаСкладе`.
    pub warehouse_stock: Vec<WarehouseStock>,
    /// `Цены/Цена`.
    pub prices: Vec<Price>,
    /// `Количество`.
    pub quantity: Option<Decimal>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn offer_ids() {
        let mut o = Offer::default();
        o.product.id = "p1#c1".into();
        assert_eq!(
            (o.id(), o.product_id(), o.characteristic_id()),
            ("p1#c1", "p1", Some("c1"))
        );
    }

    #[test]
    fn total_quantity_sources() {
        let mut o = Offer::default();
        assert_eq!(o.total_quantity(), None);
        o.warehouse_stock = vec![
            WarehouseStock {
                warehouse_id: "a".into(),
                quantity: Some(d("1.5")),
            },
            WarehouseStock {
                warehouse_id: "b".into(),
                quantity: Some(d("2")),
            },
        ];
        assert_eq!(o.total_quantity(), Some(d("3.5")));
        o.rests = vec![Rest {
            quantity: None,
            warehouse: Some(WarehouseStock {
                warehouse_id: "a".into(),
                quantity: Some(d("7")),
            }),
        }];
        assert_eq!(o.total_quantity(), Some(d("7")));
        o.quantity = Some(d("10"));
        assert_eq!(o.total_quantity(), Some(d("10")));
    }

    #[test]
    fn price_lookup() {
        let o = Offer {
            prices: vec![Price {
                price_type_id: "retail".into(),
                ..Price::default()
            }],
            ..Offer::default()
        };
        assert!(o.price("retail").is_some());
        assert!(o.price("wholesale").is_none());
    }
}
