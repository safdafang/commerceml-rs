use super::common::{PropertyValueType, RawElement, RequiredFor, Tax, Warehouse};
use super::counterparty::{Counterparty, Signatory};

/// The classifier: `Классификатор`.
///
/// It describes the structure the catalog follows: product groups,
/// properties, price types and (de facto) warehouses, units and categories.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Classifier {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: String,
    /// `Владелец` — required by the XSD, often present.
    pub owner: Option<Counterparty>,
    /// `Описание`.
    pub description: Option<String>,
    /// `Группы/Группа` — the group tree.
    pub groups: Vec<Group>,
    /// `Свойства/Свойство`.
    pub properties: Vec<Property>,
    /// `ТипыЦен/ТипЦены`.
    pub price_types: Vec<PriceType>,
    /// `Склады/Склад` (de facto).
    pub warehouses: Vec<Warehouse>,
    /// `ЕдиницыИзмерения/ЕдиницаИзмерения` (de facto).
    pub units: Vec<Unit>,
    /// `Категории/Категория` (de facto).
    pub categories: Vec<Category>,
    /// `Подписанты/Подписант`.
    pub signatories: Vec<Signatory>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Classifier {
    /// Visits every group of the tree depth-first.
    pub fn all_groups(&self) -> impl Iterator<Item = &Group> {
        let mut stack: Vec<&Group> = self.groups.iter().rev().collect();
        std::iter::from_fn(move || {
            let g = stack.pop()?;
            stack.extend(g.groups.iter().rev());
            Some(g)
        })
    }

    /// Finds a group anywhere in the tree by `Ид`.
    pub fn group(&self, id: &str) -> Option<&Group> {
        self.all_groups().find(|g| g.id == id)
    }

    /// Finds a property by `Ид`.
    pub fn property(&self, id: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }

    /// Finds a price type by `Ид`.
    pub fn price_type(&self, id: &str) -> Option<&PriceType> {
        self.price_types.iter().find(|p| p.id == id)
    }
}

/// A product group: `Группа`. Groups nest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Group {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: String,
    /// `Описание`.
    pub description: Option<String>,
    /// `Свойства/Свойство` — properties of products in this group.
    pub properties: Vec<Property>,
    /// `Группы/Группа` — subgroups.
    pub groups: Vec<Group>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// A product property: `Свойство`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Property {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: String,
    /// `Описание`.
    pub description: Option<String>,
    /// `Обязательное`.
    pub required_for: Option<RequiredFor>,
    /// `Множественное` — whether a product may have several values.
    pub multiple: Option<bool>,
    /// `ТипЗначений`.
    pub value_type: Option<PropertyValueType>,
    /// `ВариантыЗначений/Значение` — allowed plain values.
    pub values: Vec<String>,
    /// `ВариантыЗначений/Справочник` — allowed values with ids, for
    /// [`PropertyValueType::Dictionary`].
    pub dictionary: Vec<DictionaryValue>,
    /// `ДляТоваров`.
    pub for_products: Option<bool>,
    /// `ДляПредложений`.
    pub for_offers: Option<bool>,
    /// `ДляДокументов`.
    pub for_documents: Option<bool>,
    /// `Внешний` (de facto, Bitrix): values are stored in a reference book.
    pub external: Option<bool>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Property {
    /// Resolves a value id from [`dictionary`](Self::dictionary) to its text.
    pub fn dictionary_value(&self, value_id: &str) -> Option<&str> {
        self.dictionary
            .iter()
            .find(|v| v.value_id.as_deref() == Some(value_id))
            .map(|v| v.value.as_str())
    }
}

/// One allowed value of a dictionary property: `Справочник`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DictionaryValue {
    /// `ИдЗначения`.
    pub value_id: Option<String>,
    /// `Значение`.
    pub value: String,
    /// `Картинка` (de facto, Bitrix).
    pub picture: Option<String>,
}

/// A price type: `ТипЦены`, e.g. retail or wholesale.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PriceType {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: Option<String>,
    /// `Валюта` — ISO 4217 code or `руб`.
    pub currency: Option<String>,
    /// `Описание`.
    pub description: Option<String>,
    /// `Налог`.
    pub taxes: Vec<Tax>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// A unit of measure from the classifier: `ЕдиницаИзмерения` (de facto).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Unit {
    /// `Ид`.
    pub id: Option<String>,
    /// `Код` — OKEI code, e.g. `796`.
    pub code: Option<String>,
    /// `НаименованиеПолное`, e.g. `Штука`.
    pub full_name: Option<String>,
    /// `НаименованиеКраткое`, e.g. `шт`.
    pub short_name: Option<String>,
    /// `МеждународноеСокращение`, e.g. `PCE`.
    pub international_code: Option<String>,
    /// `ПометкаУдаления`.
    pub deletion_mark: Option<bool>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

/// A product category: `Категория` (de facto).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Category {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: String,
    /// `Описание`.
    pub description: Option<String>,
    /// `ПометкаУдаления`.
    pub deletion_mark: Option<bool>,
    /// Elements not modelled by the library, such as `Свойства`.
    pub extra: Vec<RawElement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, children: Vec<Group>) -> Group {
        Group {
            id: id.into(),
            groups: children,
            ..Group::default()
        }
    }

    #[test]
    fn walks_group_tree_depth_first() {
        let c = Classifier {
            groups: vec![
                group(
                    "a",
                    vec![group("a1", vec![group("a1x", vec![])]), group("a2", vec![])],
                ),
                group("b", vec![]),
            ],
            ..Classifier::default()
        };
        let ids: Vec<_> = c.all_groups().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["a", "a1", "a1x", "a2", "b"]);
        assert!(c.group("a1x").is_some());
        assert!(c.group("zzz").is_none());
    }

    #[test]
    fn dictionary_lookup() {
        let p = Property {
            dictionary: vec![DictionaryValue {
                value_id: Some("v1".into()),
                value: "Красный".into(),
                picture: None,
            }],
            ..Property::default()
        };
        assert_eq!(p.dictionary_value("v1"), Some("Красный"));
        assert_eq!(p.dictionary_value("v2"), None);
        let c = Classifier {
            properties: vec![Property { id: "p".into(), ..p }],
            price_types: vec![PriceType {
                id: "t".into(),
                ..PriceType::default()
            }],
            ..Classifier::default()
        };
        assert!(c.property("p").is_some());
        assert!(c.price_type("t").is_some());
    }
}
