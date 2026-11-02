use super::common::{ProductStatus, RawElement, Requisite};
use super::counterparty::{Counterparty, Signatory};
use crate::Decimal;

/// The catalog: `Каталог`, the list of products.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Catalog {
    /// `Ид`.
    pub id: String,
    /// `ИдКлассификатора` — the classifier the products follow.
    pub classifier_id: Option<String>,
    /// `Наименование`.
    pub name: String,
    /// `Владелец`.
    pub owner: Option<Counterparty>,
    /// `Товары/Товар`.
    pub products: Vec<Product>,
    /// `Описание`.
    pub description: Option<String>,
    /// `Подписанты/Подписант`.
    pub signatories: Vec<Signatory>,
    /// Attribute `СодержитТолькоИзменения` — an incremental export.
    pub changes_only: Option<bool>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// A product: `Товар`.
///
/// The same element type is the base of offers ([`Offer`](crate::Offer)),
/// document lines ([`DocumentItem`](crate::DocumentItem)), components and
/// analogs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Product {
    /// `Ид`. For offers it can be `<product id>#<characteristic id>`.
    pub id: String,
    /// `Штрихкод`.
    pub barcode: Option<String>,
    /// `Артикул` — the vendor code.
    pub article: Option<String>,
    /// `Наименование`.
    pub name: String,
    /// `БазоваяЕдиница`.
    pub base_unit: Option<BaseUnit>,
    /// `ИдТовараУКонтрагента` — the id the counterparty uses.
    pub counterparty_product_id: Option<String>,
    /// `Группы/Ид` — groups of the classifier.
    pub group_ids: Vec<String>,
    /// `Категория` (de facto) — id of a classifier category.
    pub category_id: Option<String>,
    /// `Описание`.
    pub description: Option<String>,
    /// `Картинка` — paths relative to the export, e.g. `import_files/ab/abcd.jpg`.
    pub pictures: Vec<String>,
    /// `Страна` — country of manufacture.
    pub country: Option<String>,
    /// `ТорговаяМарка`.
    pub trademark: Option<String>,
    /// `ВладелецТорговойМарки`.
    pub trademark_owner: Option<Box<Counterparty>>,
    /// `Изготовитель`.
    pub manufacturer: Option<Box<Counterparty>>,
    /// `ЗначенияСвойств/ЗначенияСвойства`.
    pub property_values: Vec<PropertyValue>,
    /// `СтавкиНалогов/СтавкаНалога`.
    pub tax_rates: Vec<TaxRate>,
    /// `Акцизы/Акциз`.
    pub excises: Vec<Excise>,
    /// `Комплектующие/Комплектующее` — parts of a kit.
    pub components: Vec<RelatedProduct>,
    /// `Аналоги/Аналог`.
    pub analogs: Vec<RelatedProduct>,
    /// `ХарактеристикиТовара/ХарактеристикаТовара`.
    pub characteristics: Vec<Characteristic>,
    /// `ЗначенияРеквизитов/ЗначениеРеквизита`.
    pub requisites: Vec<Requisite>,
    /// Attribute `Статус`.
    pub status: Option<ProductStatus>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// `Вес` (de facto).
    pub weight: Option<Decimal>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Product {
    /// Value of the property with this `Ид`, if set (the first value when
    /// there are several).
    pub fn property_value(&self, property_id: &str) -> Option<&str> {
        self.property_values
            .iter()
            .find(|v| v.id == property_id)
            .and_then(|v| v.values.first())
            .map(String::as_str)
    }

    /// Value of the requisite with this name (`ЗначениеРеквизита`).
    pub fn requisite(&self, name: &str) -> Option<&str> {
        crate::RequisitesExt::requisite(self.requisites.as_slice(), name)
    }

    /// Whether the product is marked as deleted, either by `Статус="Удален"`
    /// or by `ПометкаУдаления`.
    pub fn is_deleted(&self) -> bool {
        self.status == Some(ProductStatus::Deleted) || self.deletion_mark == Some(true)
    }
}

/// The base unit of measure: `БазоваяЕдиница`.
///
/// The unit is described by attributes; the element text (e.g. `шт` in
/// orders) and conversions to other units are optional.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BaseUnit {
    /// Attribute `Код` — OKEI code, e.g. `796`.
    pub code: Option<String>,
    /// Attribute `НаименованиеПолное`, e.g. `Штука`.
    pub full_name: Option<String>,
    /// Attribute `МеждународноеСокращение`, e.g. `PCE`.
    pub international_code: Option<String>,
    /// Attribute `НаименованиеКраткое`, e.g. `шт`.
    pub short_name: Option<String>,
    /// Element text.
    pub text: Option<String>,
    /// `Пересчет`.
    pub conversions: Vec<UnitConversion>,
}

/// A unit conversion: `Пересчет` (and the `ЕдиницаИзмерения` group).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnitConversion {
    /// `Единица` — unit code.
    pub unit: Option<String>,
    /// `Коэффициент` — how many base units in one `unit`.
    pub coefficient: Option<String>,
    /// `ДополнительныеДанные/ЗначениеРеквизита`.
    pub extra_data: Vec<Requisite>,
}

/// Values of one property for a product: `ЗначенияСвойства`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertyValue {
    /// `Ид` — the [`Property`](crate::Property) id.
    pub id: String,
    /// `Наименование`.
    pub name: Option<String>,
    /// `Значение` — text, or a dictionary value id.
    pub values: Vec<String>,
}

/// A tax rate of a product: `СтавкаНалога`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TaxRate {
    /// `Наименование`, e.g. `НДС`.
    pub name: String,
    /// `Ставка`, e.g. `20` or `Без налога` — a string by the standard.
    pub rate: String,
}

/// An excise: `Акциз`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Excise {
    /// `Наименование`.
    pub name: String,
    /// `СуммаЗаЕдиницу`.
    pub amount_per_unit: Option<Decimal>,
    /// `Валюта`.
    pub currency: String,
}

/// A component or an analog: `Комплектующее` / `Аналог`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RelatedProduct {
    /// The product fields.
    pub product: Box<Product>,
    /// `ИдКаталога`.
    pub catalog_id: Option<String>,
    /// `ИдКлассификатора`.
    pub classifier_id: Option<String>,
    /// `Количество` — only for components.
    pub quantity: Option<Decimal>,
}

/// A characteristic of a product variant: `ХарактеристикаТовара`
/// (size, colour…).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Characteristic {
    /// `Ид`.
    pub id: Option<String>,
    /// `Наименование`.
    pub name: String,
    /// `Значение`.
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let p = Product {
            property_values: vec![PropertyValue {
                id: "color".into(),
                name: None,
                values: vec!["red".into(), "blue".into()],
            }],
            requisites: vec![Requisite::new("ВидНоменклатуры", "Товар")],
            status: Some(ProductStatus::Changed),
            ..Product::default()
        };
        assert_eq!(p.property_value("color"), Some("red"));
        assert_eq!(p.property_value("size"), None);
        assert_eq!(p.requisite("ВидНоменклатуры"), Some("Товар"));
        assert!(!p.is_deleted());
        let deleted = Product {
            deletion_mark: Some(true),
            ..Product::default()
        };
        assert!(deleted.is_deleted());
    }
}
