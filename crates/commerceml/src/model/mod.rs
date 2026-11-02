//! The CommerceML data model.
//!
//! Type and field names are English; each item's documentation gives the
//! Russian XML element it corresponds to. A table of all mappings is in the
//! crate-level documentation.

#[macro_use]
mod common;
mod catalog;
mod classifier;
mod counterparty;
mod document;
mod offers;

pub use catalog::{
    BaseUnit, Catalog, Characteristic, Excise, Product, PropertyValue, RelatedProduct, TaxRate,
    UnitConversion,
};
pub use classifier::{Category, Classifier, DictionaryValue, Group, PriceType, Property, Unit};
pub use common::{
    Address, AddressField, AddressFieldType, BusinessOperation, Contact, ContactType, Gender, ProductStatus,
    PropertyValueType, RawElement, RequiredFor, Requisite, RequisitesExt, Role, Tax, Warehouse,
};
pub use counterparty::{Bank, BankAccount, Counterparty, Head, IdentityDocument, Representative, Signatory};
pub use document::{Adjustment, Document, DocumentItem, DocumentTax, SitePayment, WarehouseQuantity};
pub use offers::{
    Offer, OfferChange, OfferPackage, OfferPackageChanges, Price, Rest, WarehouseStock, split_offer_id,
};

/// The schema version written by default.
pub const DEFAULT_SCHEMA_VERSION: &str = "2.10";

/// The root element: `КоммерческаяИнформация`.
///
/// A file usually holds one kind of content: a classifier and catalog
/// (`import.xml`), an offer package (`offers.xml`), or documents (orders).
/// All parts are optional here, and any combination is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommercialInfo {
    /// Attribute `ВерсияСхемы`, e.g. `2.10`.
    pub schema_version: String,
    /// Attribute `ДатаФормирования` — when the file was generated.
    pub generated_at: String,
    /// `Классификатор`.
    pub classifier: Option<Classifier>,
    /// `Каталог`.
    pub catalog: Option<Catalog>,
    /// `ПакетПредложений`.
    pub offer_package: Option<OfferPackage>,
    /// `ИзмененияПакетаПредложений`.
    pub offer_package_changes: Vec<OfferPackageChanges>,
    /// `Документ`, including documents wrapped in `Контейнер` (CommerceML 3).
    pub documents: Vec<Document>,
    /// `Контрагенты/Контрагент` at the root (de facto, `contragents.xml`).
    pub counterparties: Vec<Counterparty>,
    /// Root-level elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Default for CommercialInfo {
    fn default() -> Self {
        Self {
            schema_version: DEFAULT_SCHEMA_VERSION.to_owned(),
            generated_at: String::new(),
            classifier: None,
            catalog: None,
            offer_package: None,
            offer_package_changes: Vec::new(),
            documents: Vec::new(),
            counterparties: Vec::new(),
            extra: Vec::new(),
        }
    }
}

impl CommercialInfo {
    /// An empty document with schema version 2.10 and the given generation
    /// timestamp (`xsd:dateTime`, e.g. `2026-11-02T12:00:00`).
    pub fn new(generated_at: impl Into<String>) -> Self {
        Self {
            generated_at: generated_at.into(),
            ..Self::default()
        }
    }

    /// Products of the catalog, or an empty slice.
    pub fn products(&self) -> &[Product] {
        self.catalog.as_ref().map_or(&[], |c| &c.products)
    }

    /// Offers of the offer package, or an empty slice.
    pub fn offers(&self) -> &[Offer] {
        self.offer_package.as_ref().map_or(&[], |p| &p.offers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_on_empty() {
        let info = CommercialInfo::new("2026-01-01T00:00:00");
        assert_eq!(info.schema_version, "2.10");
        assert_eq!(info.products(), []);
        assert_eq!(info.offers(), []);
    }
}
