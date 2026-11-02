//! Semantic checks beyond well-formedness.
//!
//! The parser accepts anything that is well-formed XML. [`Validator`] then
//! reports what would make the data unusable: missing required fields,
//! duplicate ids, and references to groups, properties, price types or
//! warehouses that the file does not define. It works on stream events, so
//! it can check files of any size; memory grows only with the number of ids.
//!
//! ```
//! let xml = r#"<КоммерческаяИнформация ВерсияСхемы="2.10" ДатаФормирования="2026-11-02T10:00:00">
//!   <Классификатор><Ид>k</Ид><Наименование>К</Наименование>
//!     <Группы><Группа><Ид>g1</Ид><Наименование>Группа</Наименование></Группа></Группы>
//!   </Классификатор>
//!   <Каталог><Ид>c</Ид><Наименование>К</Наименование><Товары>
//!     <Товар><Ид>p</Ид><Наименование>Т</Наименование><Группы><Ид>g2</Ид></Группы></Товар>
//!   </Товары></Каталог>
//! </КоммерческаяИнформация>"#;
//! let info = commerceml::from_str(xml)?.info;
//! let issues = commerceml::validate::validate(&info);
//! assert_eq!(issues.len(), 1);
//! assert_eq!(issues[0].code, commerceml::validate::IssueCode::UnknownReference);
//! # Ok::<(), commerceml::Error>(())
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::model::{
    Catalog, CommercialInfo, Document, Group, Offer, OfferChange, OfferPackage, PriceType, Product, Property,
    PropertyValueType, Warehouse,
};
use crate::stream::{Event, Header};

/// How serious an [`Issue`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Severity {
    /// Suspicious, but the data can be used.
    Warning,
    /// The data breaks the standard or cannot be imported as is.
    Error,
}

/// Category of an [`Issue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum IssueCode {
    /// A required field is empty or missing.
    MissingField,
    /// Two entities of the same kind share an `Ид`.
    DuplicateId,
    /// An id refers to something not defined in the file.
    UnknownReference,
    /// A value has the wrong form (date, negative price…).
    InvalidValue,
    /// `ВерсияСхемы` is outside the supported range.
    UnsupportedVersion,
}

impl IssueCode {
    /// A stable kebab-case name, e.g. `unknown-reference`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingField => "missing-field",
            Self::DuplicateId => "duplicate-id",
            Self::UnknownReference => "unknown-reference",
            Self::InvalidValue => "invalid-value",
            Self::UnsupportedVersion => "unsupported-version",
        }
    }
}

/// A problem found by the [`Validator`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Issue {
    /// How serious it is.
    pub severity: Severity,
    /// What kind of problem.
    pub code: IssueCode,
    /// The entity, e.g. `Товар "p-1"`.
    pub entity: String,
    /// Human-readable description.
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(
            f,
            "{sev}[{}] {}: {}",
            self.code.as_str(),
            self.entity,
            self.message
        )
    }
}

/// Checks a stream of events. Feed every event to [`event`](Self::event),
/// then call [`finish`](Self::finish).
#[derive(Debug, Default)]
pub struct Validator {
    issues: Vec<Issue>,
    groups: HashSet<String>,
    properties: HashMap<String, Option<HashSet<String>>>,
    price_types: HashSet<String>,
    warehouses: HashSet<String>,
    products: HashSet<String>,
    offers: HashSet<String>,
    documents: HashSet<String>,
    /// References checked at the end, when all definitions are known.
    deferred: Vec<Deferred>,
}

#[derive(Debug)]
struct Deferred {
    kind: RefKind,
    id: String,
    entity: String,
}

#[derive(Debug, Clone, Copy)]
enum RefKind {
    Group,
    Property,
    PriceType,
    Warehouse,
}

fn entity(kind: &str, id: &str) -> String {
    format!("{kind} \"{id}\"")
}

/// `YYYY-MM-DD`.
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// `YYYY-MM-DDThh:mm:ss` with optional fraction and zone.
fn is_datetime(s: &str) -> bool {
    s.len() >= 19
        && is_date(&s[..10])
        && s.as_bytes()[10] == b'T'
        && s[11..19].bytes().enumerate().all(|(i, c)| {
            if i == 2 || i == 5 {
                c == b':'
            } else {
                c.is_ascii_digit()
            }
        })
}

impl Validator {
    /// A validator with no definitions yet.
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, severity: Severity, code: IssueCode, entity: String, message: impl Into<String>) {
        self.issues.push(Issue {
            severity,
            code,
            entity,
            message: message.into(),
        });
    }

    fn require(&mut self, value: &str, field: &str, entity: &str) {
        if value.trim().is_empty() {
            self.push(
                Severity::Error,
                IssueCode::MissingField,
                entity.to_owned(),
                format!("<{field}> is empty or missing"),
            );
        }
    }

    fn unique(&mut self, kind: &str, id: &str, which: fn(&mut Self) -> &mut HashSet<String>) {
        if id.is_empty() {
            return;
        }
        if !which(self).insert(id.to_owned()) {
            self.push(
                Severity::Error,
                IssueCode::DuplicateId,
                entity(kind, id),
                format!("{kind} with this Ид occurs more than once"),
            );
        }
    }

    fn reference(&mut self, kind: RefKind, id: &str, entity: &str) {
        self.deferred.push(Deferred {
            kind,
            id: id.to_owned(),
            entity: entity.to_owned(),
        });
    }

    /// Checks one event.
    pub fn event(&mut self, event: &Event) {
        match event {
            Event::Start(h) => self.header(h),
            Event::ClassifierEnd(c) => {
                let e = entity("Классификатор", &c.id);
                self.require(&c.id, "Ид", &e);
                self.require(&c.name, "Наименование", &e);
            }
            Event::Group(g) => self.group(g),
            Event::Property(p) => self.property(p),
            Event::PriceType(t) => self.price_type(t),
            Event::Warehouse(w) => self.warehouse(w),
            Event::CatalogEnd(c) => {
                let e = entity("Каталог", &c.id);
                self.require(&c.id, "Ид", &e);
                self.require(&c.name, "Наименование", &e);
            }
            Event::Product(p) => self.product(p),
            Event::OfferPackageStart(p) => {
                for t in &p.price_types {
                    self.price_type(t);
                }
                for w in &p.warehouses {
                    self.warehouse(w);
                }
            }
            Event::OfferPackageEnd(p) => {
                let e = entity("ПакетПредложений", p.id.as_deref().unwrap_or(""));
                self.require(&p.name, "Наименование", &e);
                self.require(&p.catalog_id, "ИдКаталога", &e);
            }
            Event::Offer(o) => self.offer(o),
            Event::OfferChange(o) => self.offer_change(o),
            Event::Document(d) => self.document(d, "Документ"),
            _ => {}
        }
    }

    fn header(&mut self, h: &Header) {
        let e = "КоммерческаяИнформация".to_owned();
        self.require(&h.schema_version, "@ВерсияСхемы", &e);
        if !h.generated_at.is_empty() && !is_datetime(&h.generated_at) {
            self.push(
                Severity::Warning,
                IssueCode::InvalidValue,
                e.clone(),
                format!("ДатаФормирования {:?} is not an xsd:dateTime", h.generated_at),
            );
        }
        let v = h.schema_version.trim();
        let known = v
            .strip_prefix("2.")
            .and_then(|minor| minor.parse::<u32>().ok())
            .is_some_and(|m| (3..=10).contains(&m) && v.len() == 4);
        if !v.is_empty() && !known {
            self.push(
                Severity::Warning,
                IssueCode::UnsupportedVersion,
                e,
                format!("ВерсияСхемы {v:?} is outside 2.03–2.10; parsing is best effort"),
            );
        }
    }

    fn group(&mut self, g: &Group) {
        let e = entity("Группа", &g.id);
        self.require(&g.id, "Ид", &e);
        self.require(&g.name, "Наименование", &e);
        self.unique("Группа", &g.id, |s| &mut s.groups);
        for p in &g.properties {
            if !p.id.is_empty() {
                self.properties.entry(p.id.clone()).or_insert(None);
            }
        }
        for child in &g.groups {
            self.group(child);
        }
    }

    fn property(&mut self, p: &Property) {
        let e = entity("Свойство", &p.id);
        self.require(&p.id, "Ид", &e);
        self.require(&p.name, "Наименование", &e);
        let dictionary = (p.value_type == Some(PropertyValueType::Dictionary) && !p.dictionary.is_empty())
            .then(|| p.dictionary.iter().filter_map(|d| d.value_id.clone()).collect());
        if p.id.is_empty() {
            return;
        }
        if let std::collections::hash_map::Entry::Vacant(slot) = self.properties.entry(p.id.clone()) {
            slot.insert(dictionary);
        } else {
            self.push(
                Severity::Error,
                IssueCode::DuplicateId,
                e,
                "Свойство with this Ид occurs more than once",
            );
        }
    }

    fn price_type(&mut self, t: &PriceType) {
        let e = entity("ТипЦены", &t.id);
        self.require(&t.id, "Ид", &e);
        self.price_types.insert(t.id.clone());
    }

    fn warehouse(&mut self, w: &Warehouse) {
        let e = entity("Склад", &w.id);
        self.require(&w.id, "Ид", &e);
        self.warehouses.insert(w.id.clone());
    }

    fn product_refs(&mut self, p: &Product, e: &str) {
        for g in &p.group_ids {
            self.reference(RefKind::Group, g, e);
        }
        for v in &p.property_values {
            self.reference(RefKind::Property, &v.id, e);
            for value in &v.values {
                self.deferred.push(Deferred {
                    kind: RefKind::Property,
                    id: format!("{}\u{0}{value}", v.id),
                    entity: e.to_owned(),
                });
            }
        }
    }

    fn product(&mut self, p: &Product) {
        let e = entity("Товар", &p.id);
        self.require(&p.name, "Наименование", &e);
        if p.id.is_empty() && p.barcode.is_none() && p.article.is_none() {
            self.push(
                Severity::Error,
                IssueCode::MissingField,
                e.clone(),
                "the product has no Ид, Штрихкод or Артикул",
            );
        }
        self.unique("Товар", &p.id, |s| &mut s.products);
        self.product_refs(p, &e);
    }

    fn offer(&mut self, o: &Offer) {
        let e = entity("Предложение", o.id());
        self.require(o.id(), "Ид", &e);
        self.unique("Предложение", o.id(), |s| &mut s.offers);
        for price in &o.prices {
            self.require(&price.price_type_id, "ИдТипаЦены", &e);
            self.reference(RefKind::PriceType, &price.price_type_id, &e);
            match price.price_per_unit {
                None => self.push(
                    Severity::Error,
                    IssueCode::MissingField,
                    e.clone(),
                    format!("price of type {:?} has no ЦенаЗаЕдиницу", price.price_type_id),
                ),
                Some(v) if v.is_negative() => self.push(
                    Severity::Warning,
                    IssueCode::InvalidValue,
                    e.clone(),
                    format!("negative price {v}"),
                ),
                Some(_) => {}
            }
        }
        for s in &o.warehouse_stock {
            self.reference(RefKind::Warehouse, &s.warehouse_id, &e);
        }
        for r in &o.rests {
            if let Some(w) = &r.warehouse {
                self.reference(RefKind::Warehouse, &w.warehouse_id, &e);
            }
        }
        if let Some(q) = o.quantity.filter(|q| q.is_negative()) {
            self.push(
                Severity::Warning,
                IssueCode::InvalidValue,
                e.clone(),
                format!("negative stock {q}"),
            );
        }
        self.product_refs(&o.product, &e);
    }

    fn offer_change(&mut self, o: &OfferChange) {
        let id = o.id.as_deref().unwrap_or("");
        let e = entity("Предложение", id);
        if o.id.is_none() && o.barcode.is_none() && o.article.is_none() {
            self.push(
                Severity::Error,
                IssueCode::MissingField,
                e.clone(),
                "no Ид, Штрихкод or Артикул",
            );
        }
        for price in &o.prices {
            self.reference(RefKind::PriceType, &price.price_type_id, &e);
        }
    }

    fn document(&mut self, d: &Document, kind: &str) {
        let e = entity(kind, &d.id);
        self.require(&d.id, "Ид", &e);
        self.require(&d.number, "Номер", &e);
        self.require(&d.date, "Дата", &e);
        if kind == "Документ" {
            self.unique("Документ", &d.id, |s| &mut s.documents);
        }
        if d.operation.is_none() {
            self.push(
                Severity::Error,
                IssueCode::MissingField,
                e.clone(),
                "<ХозОперация> is missing",
            );
        }
        if !d.date.is_empty() && !is_date(&d.date) {
            self.push(
                Severity::Warning,
                IssueCode::InvalidValue,
                e.clone(),
                format!("Дата {:?} is not YYYY-MM-DD", d.date),
            );
        }
        for item in &d.items {
            if item.product.name.is_empty() {
                self.push(
                    Severity::Error,
                    IssueCode::MissingField,
                    e.clone(),
                    format!("line {:?} has no Наименование", item.product.id),
                );
            }
        }
        for s in &d.subordinate_documents {
            self.document(s, "ПодчиненныйДокумент");
        }
    }

    /// Resolves deferred references and returns all issues.
    pub fn finish(mut self) -> Vec<Issue> {
        let deferred = std::mem::take(&mut self.deferred);
        for d in deferred {
            let (known, what) = match d.kind {
                RefKind::Group => (self.groups.is_empty() || self.groups.contains(&d.id), "Группа"),
                RefKind::PriceType => (
                    self.price_types.is_empty() || self.price_types.contains(&d.id),
                    "ТипЦены",
                ),
                RefKind::Warehouse => (
                    self.warehouses.is_empty() || self.warehouses.contains(&d.id),
                    "Склад",
                ),
                RefKind::Property => {
                    if let Some((prop, value)) = d.id.split_once('\u{0}') {
                        let ok = match self.properties.get(prop) {
                            Some(Some(ids)) => ids.contains(value) || value.is_empty(),
                            _ => true,
                        };
                        if !ok {
                            self.push(
                                Severity::Warning,
                                IssueCode::UnknownReference,
                                d.entity,
                                format!("value {value:?} is not in the dictionary of Свойство {prop:?}"),
                            );
                        }
                        continue;
                    }
                    (
                        self.properties.is_empty() || self.properties.contains_key(&d.id),
                        "Свойство",
                    )
                }
            };
            if !known {
                self.push(
                    Severity::Error,
                    IssueCode::UnknownReference,
                    d.entity,
                    format!("refers to {what} {:?} that is not defined in the file", d.id),
                );
            }
        }
        self.issues
    }
}

/// Validates a whole document.
pub fn validate(info: &CommercialInfo) -> Vec<Issue> {
    let mut v = Validator::new();
    v.event(&Event::Start(Header {
        schema_version: info.schema_version.clone(),
        generated_at: info.generated_at.clone(),
    }));
    if let Some(c) = &info.classifier {
        for g in &c.groups {
            v.group(g);
        }
        for p in &c.properties {
            v.property(p);
        }
        for t in &c.price_types {
            v.price_type(t);
        }
        for w in &c.warehouses {
            v.warehouse(w);
        }
        v.event(&Event::ClassifierEnd(c.clone()));
    }
    if let Some(c) = &info.catalog {
        for p in &c.products {
            v.product(p);
        }
        v.event(&Event::CatalogEnd(catalog_header(c)));
    }
    if let Some(p) = &info.offer_package {
        v.event(&Event::OfferPackageStart(package_header(p)));
        for o in &p.offers {
            v.offer(o);
        }
        v.event(&Event::OfferPackageEnd(package_header(p)));
    }
    for c in &info.offer_package_changes {
        for o in &c.offers {
            v.offer_change(o);
        }
    }
    for d in &info.documents {
        v.document(d, "Документ");
    }
    v.finish()
}

/// A copy of a catalog header without products.
fn catalog_header(c: &Catalog) -> Catalog {
    Catalog {
        id: c.id.clone(),
        name: c.name.clone(),
        ..Catalog::default()
    }
}

/// A copy of an offer package header without offers.
fn package_header(p: &OfferPackage) -> OfferPackage {
    OfferPackage {
        id: p.id.clone(),
        name: p.name.clone(),
        catalog_id: p.catalog_id.clone(),
        price_types: p.price_types.clone(),
        warehouses: p.warehouses.clone(),
        ..OfferPackage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_forms() {
        assert!(is_date("2026-11-02"));
        assert!(!is_date("02.11.2026"));
        assert!(is_datetime("2026-11-02T10:00:00"));
        assert!(is_datetime("2026-11-02T10:00:00.5+03:00"));
        assert!(!is_datetime("2026-11-02 10:00:00"));
    }

    #[test]
    fn display() {
        let i = Issue {
            severity: Severity::Error,
            code: IssueCode::DuplicateId,
            entity: entity("Товар", "1"),
            message: "dup".into(),
        };
        assert_eq!(i.to_string(), "error[duplicate-id] Товар \"1\": dup");
    }
}
