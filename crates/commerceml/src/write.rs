//! Writing CommerceML.
//!
//! [`to_string`](crate::to_string) and friends serialize a whole
//! [`CommercialInfo`]. [`Writer`] writes the same output piece by piece, for
//! exports too large to build in memory:
//!
//! ```
//! use commerceml::stream::Header;
//! use commerceml::write::{WriteOptions, Writer};
//! use commerceml::{Catalog, Product};
//!
//! let header = Header { schema_version: "2.10".into(), generated_at: "2026-11-02T10:00:00".into() };
//! let mut w = Writer::new(Vec::new(), &header, &WriteOptions::default())?;
//! w.begin_catalog(&Catalog { id: "cat".into(), name: "Каталог".into(), ..Catalog::default() })?;
//! for i in 0..3 {
//!     w.product(&Product { id: format!("p{i}"), name: format!("Товар {i}"), ..Product::default() })?;
//! }
//! w.end_catalog()?;
//! let xml = String::from_utf8(w.finish()?).unwrap();
//! assert!(xml.contains("<Наименование>Товар 2</Наименование>"));
//! # Ok::<(), commerceml::Error>(())
//! ```

use std::io::{self, Write};

use crate::encoding::Encoding;
use crate::error::{Error, Result};
use crate::model::{
    Address, Adjustment, Bank, BankAccount, BaseUnit, BusinessOperation, Catalog, Category, Classifier,
    CommercialInfo, Contact, Counterparty, Document, DocumentItem, DocumentTax, Gender, Group, Head,
    IdentityDocument, Offer, OfferChange, OfferPackage, OfferPackageChanges, Price, PriceType, Product,
    Property, PropertyValue, PropertyValueType, RawElement, RelatedProduct, RequiredFor, Requisite, Role,
    Signatory, Tax, Unit, UnitConversion, Warehouse, WarehouseStock,
};
use crate::stream::Header;
use crate::xml::writer::XmlWriter;

type W<T> = XmlWriter<T>;
type IoResult = io::Result<()>;

/// The namespace written by default.
pub const DEFAULT_NAMESPACE: &str = "urn:1C.ru:commerceml_2";

/// Output settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    pub(crate) encoding: Encoding,
    pub(crate) indent: bool,
    pub(crate) declaration: bool,
    pub(crate) namespace: Option<String>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            encoding: Encoding::UTF_8,
            indent: true,
            declaration: true,
            namespace: Some(DEFAULT_NAMESPACE.to_owned()),
        }
    }
}

impl WriteOptions {
    /// Output encoding (default UTF-8). Characters the encoding cannot
    /// represent are written as numeric character references (`&#NNNN;`).
    #[must_use]
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Indent with tabs, one element per line (default `true`).
    #[must_use]
    pub fn indent(mut self, indent: bool) -> Self {
        self.indent = indent;
        self
    }

    /// Write the `<?xml …?>` declaration (default `true`).
    #[must_use]
    pub fn declaration(mut self, declaration: bool) -> Self {
        self.declaration = declaration;
        self
    }

    /// Default namespace of the root element (default
    /// `urn:1C.ru:commerceml_2`); `None` writes no `xmlns`.
    #[must_use]
    pub fn namespace(mut self, namespace: Option<&str>) -> Self {
        self.namespace = namespace.map(str::to_owned);
        self
    }

    /// Serializes `info` into `out`.
    pub fn write<T: Write>(&self, info: &CommercialInfo, out: T) -> Result<T> {
        write_info(info, out, self)
    }

    /// Serializes `info` into a byte vector.
    pub fn to_vec(&self, info: &CommercialInfo) -> Result<Vec<u8>> {
        self.write(info, Vec::new())
    }
}

fn bool_text(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn address<T: Write>(w: &mut W<T>, tag: &str, a: &Address) -> IoResult {
    w.start(tag, &[])?;
    w.text("Представление", &a.presentation)?;
    w.opt("Комментарий", a.comment.as_deref())?;
    for f in &a.fields {
        w.start("АдресноеПоле", &[])?;
        w.text("Тип", f.kind.as_str())?;
        w.text("Значение", &f.value)?;
        w.end("АдресноеПоле")?;
    }
    w.end(tag)
}

fn opt_address<T: Write>(w: &mut W<T>, tag: &str, a: Option<&Address>) -> IoResult {
    a.map_or(Ok(()), |a| address(w, tag, a))
}

fn contacts<T: Write>(w: &mut W<T>, list: &[Contact]) -> IoResult {
    if list.is_empty() {
        return Ok(());
    }
    w.start("Контакты", &[])?;
    for c in list {
        w.start("Контакт", &[])?;
        w.text("Тип", c.kind.as_str())?;
        w.text("Значение", &c.value)?;
        w.opt("Комментарий", c.comment.as_deref())?;
        w.end("Контакт")?;
    }
    w.end("Контакты")
}

fn requisite<T: Write>(w: &mut W<T>, r: &Requisite) -> IoResult {
    w.start("ЗначениеРеквизита", &[])?;
    w.text("Наименование", &r.name)?;
    for v in &r.values {
        w.text("Значение", v)?;
    }
    w.end("ЗначениеРеквизита")
}

fn requisites<T: Write>(w: &mut W<T>, tag: &str, list: &[Requisite]) -> IoResult {
    if list.is_empty() {
        return Ok(());
    }
    w.start(tag, &[])?;
    for r in list {
        requisite(w, r)?;
    }
    w.end(tag)
}

fn tax_fields<T: Write>(w: &mut W<T>, t: &Tax) -> IoResult {
    w.text("Наименование", &t.name)?;
    w.boolean("УчтеноВСумме", t.included_in_amount)?;
    w.boolean("Акциз", t.excise)
}

fn warehouse_fields<T: Write>(w: &mut W<T>, s: &Warehouse) -> IoResult {
    w.text("Ид", &s.id)?;
    w.opt("Наименование", s.name.as_deref())?;
    w.opt("Комментарий", s.comment.as_deref())?;
    opt_address(w, "Адрес", s.address.as_ref())?;
    contacts(w, &s.contacts)?;
    w.boolean("ПометкаУдаления", s.deletion_mark)?;
    w.opt("НомерВерсии", s.version.as_deref())?;
    w.raw(&s.extra)
}

fn warehouse<T: Write>(w: &mut W<T>, s: &Warehouse) -> IoResult {
    w.start("Склад", &[])?;
    warehouse_fields(w, s)?;
    w.end("Склад")
}

fn list<T: Write, X>(
    w: &mut W<T>,
    tag: &str,
    items: &[X],
    mut f: impl FnMut(&mut W<T>, &X) -> IoResult,
) -> IoResult {
    if items.is_empty() {
        return Ok(());
    }
    w.start(tag, &[])?;
    for i in items {
        f(w, i)?;
    }
    w.end(tag)
}

fn identity_document<T: Write>(w: &mut W<T>, d: &IdentityDocument) -> IoResult {
    w.start("УдостоверениеЛичности", &[])?;
    w.text("ВидДокумента", &d.kind)?;
    w.opt("Серия", d.series.as_deref())?;
    w.text("Номер", &d.number)?;
    w.text("ДатаВыдачи", &d.issue_date)?;
    w.text("КемВыдан", &d.issued_by)?;
    w.end("УдостоверениеЛичности")
}

fn bank<T: Write>(w: &mut W<T>, tag: &str, b: &Bank) -> IoResult {
    w.start(tag, &[])?;
    w.opt("СчетКорреспондентский", b.correspondent_account.as_deref())?;
    w.opt("Наименование", b.name.as_deref())?;
    opt_address(w, "Адрес", b.address.as_ref())?;
    contacts(w, &b.contacts)?;
    w.opt("БИК", b.bic.as_deref())?;
    w.opt("SWIFT", b.swift.as_deref())?;
    w.end(tag)
}

fn bank_account<T: Write>(w: &mut W<T>, a: &BankAccount) -> IoResult {
    w.start("РасчетныйСчет", &[])?;
    w.text("НомерСчета", &a.number)?;
    bank(w, "Банк", &a.bank)?;
    if let Some(b) = &a.correspondent_bank {
        bank(w, "БанкКорреспондент", b)?;
    }
    w.opt("Комментарий", a.comment.as_deref())?;
    w.end("РасчетныйСчет")
}

fn head<T: Write>(w: &mut W<T>, h: &Head) -> IoResult {
    w.start("Руководитель", &[])?;
    w.text("Фамилия", &h.last_name)?;
    w.text("Имя", &h.first_name)?;
    w.opt("Отчество", h.middle_name.as_deref())?;
    if let Some(d) = &h.identity_document {
        identity_document(w, d)?;
    }
    opt_address(w, "АресРегистрации", h.registration_address.as_ref())?;
    w.text("Должность", &h.position)?;
    contacts(w, &h.contacts)?;
    w.end("Руководитель")
}

fn signatory<T: Write>(w: &mut W<T>, s: &Signatory) -> IoResult {
    w.start("Подписант", &[])?;
    w.text("Фамилия", &s.last_name)?;
    w.text("Имя", &s.first_name)?;
    w.opt("Отчество", s.middle_name.as_deref())?;
    w.opt("Обращение", s.salutation.as_deref())?;
    if let Some(d) = &s.identity_document {
        identity_document(w, d)?;
    }
    opt_address(w, "АдресРегистрации", s.registration_address.as_ref())?;
    if let Some(c) = &s.workplace {
        counterparty(w, "МестоРаботы", c)?;
    }
    w.opt("Должность", s.position.as_deref())?;
    w.opt("Комментарий", s.comment.as_deref())?;
    w.end("Подписант")
}

fn signatories<T: Write>(w: &mut W<T>, list_: &[Signatory]) -> IoResult {
    list(w, "Подписанты", list_, signatory)
}

fn counterparty_fields<T: Write>(w: &mut W<T>, c: &Counterparty) -> IoResult {
    let legal = c.is_legal_entity();
    w.opt("Ид", c.id.as_deref())?;
    w.opt("Наименование", c.name.as_deref())?;
    w.opt("ОфициальноеНаименование", c.official_name.as_deref())?;
    opt_address(w, "ЮридическийАдрес", c.legal_address.as_ref())?;
    if legal {
        w.opt("ИНН", c.inn.as_deref())?;
        w.opt("КПП", c.kpp.as_deref())?;
    }
    w.opt("ОсновнойВидДеятельности", c.main_activity.as_deref())?;
    w.opt("ЕГРПО", c.egrpo.as_deref())?;
    w.opt("ОКВЭД", c.okved.as_deref())?;
    w.opt("ОКДП", c.okdp.as_deref())?;
    w.opt("ОКОПФ", c.okopf.as_deref())?;
    w.opt("ОКФС", c.okfs.as_deref())?;
    w.opt("ОКПО", c.okpo.as_deref())?;
    w.opt("ДатаРегистрации", c.registration_date.as_deref())?;
    if let Some(h) = &c.head {
        head(w, h)?;
    }
    list(w, "РасчетныеСчета", &c.bank_accounts, bank_account)?;
    w.opt("ПолноеНаименование", c.full_name.as_deref())?;
    w.opt("Обращение", c.salutation.as_deref())?;
    w.opt("Фамилия", c.last_name.as_deref())?;
    w.opt("Имя", c.first_name.as_deref())?;
    w.opt("Отчество", c.middle_name.as_deref())?;
    w.opt("ДатаРождения", c.birth_date.as_deref())?;
    opt_address(w, "МестоРождения", c.birth_place.as_ref())?;
    w.opt("Пол", c.gender.as_ref().map(Gender::as_str))?;
    if !legal {
        w.opt("ИНН", c.inn.as_deref())?;
        w.opt("КПП", c.kpp.as_deref())?;
    }
    if let Some(d) = &c.identity_document {
        identity_document(w, d)?;
    }
    opt_address(w, "АдресРегистрации", c.registration_address.as_ref())?;
    if let Some(o) = &c.organization {
        counterparty(w, "Организация", o)?;
    }
    w.opt("Должность", c.position.as_deref())?;
    w.opt("Комментарий", c.comment.as_deref())?;
    opt_address(w, "Адрес", c.address.as_ref())?;
    contacts(w, &c.contacts)?;
    list(w, "Представители", &c.representatives, |w, r| {
        w.start("Представитель", &[])?;
        w.start("Контрагент", &[])?;
        w.text("Отношение", &r.relation)?;
        counterparty_fields(w, &r.counterparty)?;
        w.end("Контрагент")?;
        w.end("Представитель")
    })?;
    w.opt("Роль", c.role.as_ref().map(Role::as_str))?;
    if let Some(a) = &c.bank_account {
        bank_account(w, a)?;
    }
    if let Some(s) = &c.warehouse {
        warehouse(w, s)?;
    }
    w.boolean("ПометкаУдаления", c.deletion_mark)?;
    w.opt("НомерВерсии", c.version.as_deref())?;
    w.raw(&c.extra)
}

fn counterparty<T: Write>(w: &mut W<T>, tag: &str, c: &Counterparty) -> IoResult {
    w.start(tag, &[])?;
    counterparty_fields(w, c)?;
    w.end(tag)
}

fn group<T: Write>(w: &mut W<T>, g: &Group) -> IoResult {
    w.start("Группа", &[])?;
    w.text("Ид", &g.id)?;
    w.text("Наименование", &g.name)?;
    w.opt("Описание", g.description.as_deref())?;
    list(w, "Свойства", &g.properties, property)?;
    list(w, "Группы", &g.groups, group)?;
    w.boolean("ПометкаУдаления", g.deletion_mark)?;
    w.opt("НомерВерсии", g.version.as_deref())?;
    w.raw(&g.extra)?;
    w.end("Группа")
}

fn property<T: Write>(w: &mut W<T>, p: &Property) -> IoResult {
    w.start("Свойство", &[])?;
    w.text("Ид", &p.id)?;
    w.text("Наименование", &p.name)?;
    w.opt("Описание", p.description.as_deref())?;
    w.opt("Обязательное", p.required_for.as_ref().map(RequiredFor::as_str))?;
    w.boolean("Множественное", p.multiple)?;
    w.opt(
        "ТипЗначений",
        p.value_type.as_ref().map(PropertyValueType::as_str),
    )?;
    if !p.values.is_empty() || !p.dictionary.is_empty() {
        w.start("ВариантыЗначений", &[])?;
        for v in &p.values {
            w.text("Значение", v)?;
        }
        for d in &p.dictionary {
            w.start("Справочник", &[])?;
            w.opt("ИдЗначения", d.value_id.as_deref())?;
            w.text("Значение", &d.value)?;
            w.opt("Картинка", d.picture.as_deref())?;
            w.end("Справочник")?;
        }
        w.end("ВариантыЗначений")?;
    }
    w.boolean("ДляТоваров", p.for_products)?;
    w.boolean("ДляПредложений", p.for_offers)?;
    w.boolean("ДляДокументов", p.for_documents)?;
    w.boolean("Внешний", p.external)?;
    w.boolean("ПометкаУдаления", p.deletion_mark)?;
    w.opt("НомерВерсии", p.version.as_deref())?;
    w.raw(&p.extra)?;
    w.end("Свойство")
}

fn price_type<T: Write>(w: &mut W<T>, t: &PriceType) -> IoResult {
    w.start("ТипЦены", &[])?;
    w.text("Ид", &t.id)?;
    w.opt("Наименование", t.name.as_deref())?;
    w.opt("Валюта", t.currency.as_deref())?;
    w.opt("Описание", t.description.as_deref())?;
    for tax in &t.taxes {
        w.start("Налог", &[])?;
        tax_fields(w, tax)?;
        w.end("Налог")?;
    }
    w.boolean("ПометкаУдаления", t.deletion_mark)?;
    w.opt("НомерВерсии", t.version.as_deref())?;
    w.raw(&t.extra)?;
    w.end("ТипЦены")
}

fn unit<T: Write>(w: &mut W<T>, u: &Unit) -> IoResult {
    w.start("ЕдиницаИзмерения", &[])?;
    w.opt("Ид", u.id.as_deref())?;
    w.opt("Код", u.code.as_deref())?;
    w.opt("НаименованиеПолное", u.full_name.as_deref())?;
    w.opt("НаименованиеКраткое", u.short_name.as_deref())?;
    w.opt("МеждународноеСокращение", u.international_code.as_deref())?;
    w.boolean("ПометкаУдаления", u.deletion_mark)?;
    w.raw(&u.extra)?;
    w.end("ЕдиницаИзмерения")
}

fn category<T: Write>(w: &mut W<T>, c: &Category) -> IoResult {
    w.start("Категория", &[])?;
    w.text("Ид", &c.id)?;
    w.text("Наименование", &c.name)?;
    w.opt("Описание", c.description.as_deref())?;
    w.boolean("ПометкаУдаления", c.deletion_mark)?;
    w.raw(&c.extra)?;
    w.end("Категория")
}

fn classifier<T: Write>(w: &mut W<T>, c: &Classifier) -> IoResult {
    w.start("Классификатор", &[])?;
    w.text("Ид", &c.id)?;
    w.text("Наименование", &c.name)?;
    if let Some(o) = &c.owner {
        counterparty(w, "Владелец", o)?;
    }
    w.opt("Описание", c.description.as_deref())?;
    list(w, "Группы", &c.groups, group)?;
    list(w, "Свойства", &c.properties, property)?;
    list(w, "ТипыЦен", &c.price_types, price_type)?;
    list(w, "Склады", &c.warehouses, warehouse)?;
    list(w, "ЕдиницыИзмерения", &c.units, unit)?;
    list(w, "Категории", &c.categories, category)?;
    signatories(w, &c.signatories)?;
    w.raw(&c.extra)?;
    w.end("Классификатор")
}

fn unit_conversion_fields<T: Write>(w: &mut W<T>, u: &UnitConversion) -> IoResult {
    w.opt("Единица", u.unit.as_deref())?;
    w.opt("Коэффициент", u.coefficient.as_deref())?;
    requisites(w, "ДополнительныеДанные", &u.extra_data)
}

fn base_unit<T: Write>(w: &mut W<T>, u: &BaseUnit) -> IoResult {
    let mut attrs: Vec<(&str, &str)> = Vec::with_capacity(4);
    if let Some(v) = &u.code {
        attrs.push(("Код", v));
    }
    if let Some(v) = &u.full_name {
        attrs.push(("НаименованиеПолное", v));
    }
    if let Some(v) = &u.international_code {
        attrs.push(("МеждународноеСокращение", v));
    }
    if let Some(v) = &u.short_name {
        attrs.push(("НаименованиеКраткое", v));
    }
    let text = u.text.as_deref().unwrap_or("");
    w.mixed(
        "БазоваяЕдиница",
        &attrs,
        text,
        !u.conversions.is_empty(),
        |w| {
            for c in &u.conversions {
                w.start("Пересчет", &[])?;
                unit_conversion_fields(w, c)?;
                w.end("Пересчет")?;
            }
            Ok(())
        },
    )
}

fn property_value<T: Write>(w: &mut W<T>, v: &PropertyValue) -> IoResult {
    w.start("ЗначенияСвойства", &[])?;
    w.text("Ид", &v.id)?;
    w.opt("Наименование", v.name.as_deref())?;
    for x in &v.values {
        w.text("Значение", x)?;
    }
    w.end("ЗначенияСвойства")
}

fn product_attrs(p: &Product) -> Vec<(&'static str, &str)> {
    p.status
        .as_ref()
        .map(|s| ("Статус", s.as_str()))
        .into_iter()
        .collect()
}

fn product_fields<T: Write>(w: &mut W<T>, p: &Product) -> IoResult {
    w.text_nonempty("Ид", &p.id)?;
    w.opt("Штрихкод", p.barcode.as_deref())?;
    w.opt("Артикул", p.article.as_deref())?;
    w.text("Наименование", &p.name)?;
    if let Some(u) = &p.base_unit {
        base_unit(w, u)?;
    }
    w.opt("ИдТовараУКонтрагента", p.counterparty_product_id.as_deref())?;
    list(w, "Группы", &p.group_ids, |w, id| w.text("Ид", id))?;
    w.opt("Категория", p.category_id.as_deref())?;
    w.opt("Описание", p.description.as_deref())?;
    for pic in &p.pictures {
        w.text("Картинка", pic)?;
    }
    w.opt("Страна", p.country.as_deref())?;
    w.opt("ТорговаяМарка", p.trademark.as_deref())?;
    if let Some(o) = &p.trademark_owner {
        counterparty(w, "ВладелецТорговойМарки", o)?;
    }
    if let Some(m) = &p.manufacturer {
        counterparty(w, "Изготовитель", m)?;
    }
    list(w, "ЗначенияСвойств", &p.property_values, property_value)?;
    list(w, "СтавкиНалогов", &p.tax_rates, |w, t| {
        w.start("СтавкаНалога", &[])?;
        w.text("Наименование", &t.name)?;
        w.text("Ставка", &t.rate)?;
        w.end("СтавкаНалога")
    })?;
    list(w, "Акцизы", &p.excises, |w, x| {
        w.start("Акциз", &[])?;
        w.text("Наименование", &x.name)?;
        w.decimal("СуммаЗаЕдиницу", x.amount_per_unit)?;
        w.text("Валюта", &x.currency)?;
        w.end("Акциз")
    })?;
    list(w, "Комплектующие", &p.components, |w, r| {
        related(w, "Комплектующее", r)
    })?;
    list(w, "Аналоги", &p.analogs, |w, r| related(w, "Аналог", r))?;
    list(
        w,
        "ХарактеристикиТовара",
        &p.characteristics,
        |w, c| {
            w.start("ХарактеристикаТовара", &[])?;
            w.opt("Ид", c.id.as_deref())?;
            w.text("Наименование", &c.name)?;
            w.text("Значение", &c.value)?;
            w.end("ХарактеристикаТовара")
        },
    )?;
    requisites(w, "ЗначенияРеквизитов", &p.requisites)?;
    w.boolean("ПометкаУдаления", p.deletion_mark)?;
    w.opt("НомерВерсии", p.version.as_deref())?;
    w.decimal("Вес", p.weight)?;
    w.raw(&p.extra)
}

fn related<T: Write>(w: &mut W<T>, tag: &str, r: &RelatedProduct) -> IoResult {
    w.start(tag, &product_attrs(&r.product))?;
    product_fields(w, &r.product)?;
    w.opt("ИдКаталога", r.catalog_id.as_deref())?;
    w.opt("ИдКлассификатора", r.classifier_id.as_deref())?;
    w.decimal("Количество", r.quantity)?;
    w.end(tag)
}

fn product<T: Write>(w: &mut W<T>, p: &Product) -> IoResult {
    w.start("Товар", &product_attrs(p))?;
    product_fields(w, p)?;
    w.end("Товар")
}

fn price<T: Write>(w: &mut W<T>, p: &Price) -> IoResult {
    w.start("Цена", &[])?;
    w.opt("Представление", p.presentation.as_deref())?;
    w.text("ИдТипаЦены", &p.price_type_id)?;
    w.decimal("ЦенаЗаЕдиницу", p.price_per_unit)?;
    w.opt("Валюта", p.currency.as_deref())?;
    unit_conversion_fields(w, &p.unit)?;
    w.decimal("МинКоличество", p.min_quantity)?;
    w.opt("ИдКаталога", p.catalog_id.as_deref())?;
    w.end("Цена")
}

fn stock_attrs<T: Write>(w: &mut W<T>, tag: &str, s: &WarehouseStock) -> IoResult {
    let q = s.quantity.map(|q| q.to_string());
    let mut attrs = vec![("ИдСклада", s.warehouse_id.as_str())];
    if let Some(q) = &q {
        attrs.push(("КоличествоНаСкладе", q));
    }
    w.empty(tag, &attrs)
}

fn offer<T: Write>(w: &mut W<T>, o: &Offer) -> IoResult {
    w.start("Предложение", &product_attrs(&o.product))?;
    product_fields(w, &o.product)?;
    list(w, "Цены", &o.prices, price)?;
    w.decimal("Количество", o.quantity)?;
    for s in &o.warehouse_stock {
        stock_attrs(w, "Склад", s)?;
    }
    list(w, "Остатки", &o.rests, |w, r| {
        w.start("Остаток", &[])?;
        w.decimal("Количество", r.quantity)?;
        if let Some(s) = &r.warehouse {
            w.start("Склад", &[])?;
            w.text("Ид", &s.warehouse_id)?;
            w.decimal("Количество", s.quantity)?;
            w.end("Склад")?;
        }
        w.end("Остаток")
    })?;
    w.end("Предложение")
}

fn offer_change<T: Write>(w: &mut W<T>, o: &OfferChange) -> IoResult {
    w.start("Предложение", &[])?;
    w.opt("Ид", o.id.as_deref())?;
    w.opt("Штрихкод", o.barcode.as_deref())?;
    w.opt("Артикул", o.article.as_deref())?;
    w.opt("ИдХарактеристики", o.characteristic_id.as_deref())?;
    w.opt("КодЕдиницыИзмерения", o.unit_code.as_deref())?;
    for s in &o.warehouse_stock {
        stock_attrs(w, "Склады", s)?;
    }
    list(w, "Цены", &o.prices, price)?;
    w.decimal("Количество", o.quantity)?;
    w.raw(&o.extra)?;
    w.end("Предложение")
}

fn changes_attrs(changes_only: Option<bool>) -> Vec<(&'static str, &'static str)> {
    changes_only
        .map(|b| ("СодержитТолькоИзменения", bool_text(b)))
        .into_iter()
        .collect()
}

fn document_tax<T: Write>(w: &mut W<T>, t: &DocumentTax) -> IoResult {
    w.start("Налог", &[])?;
    tax_fields(w, &t.tax)?;
    w.decimal("Сумма", t.amount)?;
    w.opt("Ставка", t.rate.as_deref())?;
    w.end("Налог")
}

fn adjustment<T: Write>(w: &mut W<T>, tag: &str, a: &Adjustment) -> IoResult {
    w.start(tag, &[])?;
    w.text("Наименование", &a.name)?;
    w.decimal("Сумма", a.amount)?;
    w.opt("Процент", a.percent.as_deref())?;
    w.boolean("УчтеноВСумме", a.included_in_amount)?;
    w.opt("Комментарий", a.comment.as_deref())?;
    w.end(tag)
}

fn document_item<T: Write>(w: &mut W<T>, i: &DocumentItem) -> IoResult {
    w.start("Товар", &product_attrs(&i.product))?;
    product_fields(w, &i.product)?;
    w.opt("ИдКаталога", i.catalog_id.as_deref())?;
    w.opt("ИдКлассификатора", i.classifier_id.as_deref())?;
    w.decimal("ЦенаЗаЕдиницу", i.price_per_unit)?;
    w.decimal("Количество", i.quantity)?;
    w.decimal("Сумма", i.amount)?;
    unit_conversion_fields(w, &i.unit)?;
    w.opt("СтранаПроисхождения", i.country_of_origin.as_deref())?;
    w.opt("ГТД", i.customs_declaration.as_deref())?;
    list(w, "Налоги", &i.taxes, document_tax)?;
    list(w, "Скидки", &i.discounts, |w, a| adjustment(w, "Скидка", a))?;
    list(w, "ДопРасходы", &i.extra_charges, |w, a| {
        adjustment(w, "ДопРасход", a)
    })?;
    requisites(w, "ДополнительныеЗначенияРеквизитов", &i.additional_requisites)?;
    list(w, "Склады", &i.warehouses, |w, s| {
        w.start("Склад", &[])?;
        warehouse_fields(w, &s.warehouse)?;
        w.decimal("Количество", s.quantity)?;
        w.end("Склад")
    })?;
    w.end("Товар")
}

fn document<T: Write>(w: &mut W<T>, tag: &str, d: &Document) -> IoResult {
    w.start(tag, &[])?;
    w.text("Ид", &d.id)?;
    w.text("Номер", &d.number)?;
    w.text("Дата", &d.date)?;
    w.opt("ХозОперация", d.operation.as_ref().map(BusinessOperation::as_str))?;
    w.opt("Роль", d.role.as_ref().map(Role::as_str))?;
    w.opt("Валюта", d.currency.as_deref())?;
    w.opt("Курс", d.rate.as_deref())?;
    w.decimal("Сумма", d.amount)?;
    list(w, "Контрагенты", &d.counterparties, |w, c| {
        counterparty(w, "Контрагент", c)
    })?;
    w.opt("Время", d.time.as_deref())?;
    w.opt("СрокПлатежа", d.payment_due.as_deref())?;
    w.opt("Комментарий", d.comment.as_deref())?;
    list(w, "Налоги", &d.taxes, document_tax)?;
    list(w, "Скидки", &d.discounts, |w, a| adjustment(w, "Скидка", a))?;
    list(w, "ДопРасходы", &d.extra_charges, |w, a| {
        adjustment(w, "ДопРасход", a)
    })?;
    list(w, "Склады", &d.warehouses, warehouse)?;
    list(w, "Товары", &d.items, document_item)?;
    requisites(w, "ЗначенияРеквизитов", &d.requisites)?;
    signatories(w, &d.signatories)?;
    list(w, "Оплаты", &d.payments, |w, p| {
        w.start("Оплата", &[])?;
        w.text("НомерДокумента", &p.document_number)?;
        w.text("НомерТранзакции", &p.transaction_number)?;
        w.text("ДатаОплаты", &p.date)?;
        w.decimal("СуммаОплаты", p.amount)?;
        w.text("СпособОплаты", &p.method)?;
        w.opt("ИдСпособаОплаты", p.method_id.as_deref())?;
        w.end("Оплата")
    })?;
    list(
        w,
        "ПодчиненныеДокументы",
        &d.subordinate_documents,
        |w, s| document(w, "ПодчиненныйДокумент", s),
    )?;
    w.opt("Номер1С", d.number_1c.as_deref())?;
    w.opt("Дата1С", d.date_1c.as_deref())?;
    w.opt("Основание", d.basis.as_deref())?;
    w.boolean("ПометкаУдаления", d.deletion_mark)?;
    w.opt("НомерВерсии", d.version.as_deref())?;
    w.raw(&d.extra)?;
    w.end(tag)
}

#[derive(Debug)]
enum State {
    Root,
    Catalog(Box<Catalog>),
    OfferPackage(Box<OfferPackage>),
    Changes(Box<OfferPackageChanges>),
    Counterparties,
}

/// Writes a CommerceML document piece by piece. See the
/// [module documentation](self).
///
/// Sections must be properly nested: after `begin_catalog` only `product`
/// and `end_catalog` are allowed, and so on. Misuse returns
/// [`Error::WriterState`].
pub struct Writer<T: Write> {
    xml: W<T>,
    state: State,
}

impl<T: Write> Writer<T> {
    /// Writes the XML declaration and opens the root element.
    pub fn new(out: T, header: &Header, options: &WriteOptions) -> Result<Self> {
        let mut xml = XmlWriter::new(out, options.encoding, options.indent);
        if options.declaration {
            xml.declaration(options.encoding)?;
        }
        let mut attrs: Vec<(&str, &str)> = Vec::new();
        if let Some(ns) = &options.namespace {
            attrs.push(("xmlns", ns));
            attrs.push(("xmlns:xs", "http://www.w3.org/2001/XMLSchema"));
            attrs.push(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
        }
        attrs.push(("ВерсияСхемы", &header.schema_version));
        attrs.push(("ДатаФормирования", &header.generated_at));
        xml.start("КоммерческаяИнформация", &attrs)?;
        Ok(Self {
            xml,
            state: State::Root,
        })
    }

    fn expect_root(&self, what: &'static str) -> Result<()> {
        match self.state {
            State::Root => Ok(()),
            _ => Err(Error::WriterState(what)),
        }
    }

    /// Writes a whole `Классификатор`.
    pub fn classifier(&mut self, c: &Classifier) -> Result<()> {
        self.expect_root("classifier() inside an open section")?;
        Ok(classifier(&mut self.xml, c)?)
    }

    /// Opens `Каталог` and writes its header; `products` of `c` are ignored.
    pub fn begin_catalog(&mut self, c: &Catalog) -> Result<()> {
        self.expect_root("begin_catalog() inside an open section")?;
        let x = &mut self.xml;
        x.start("Каталог", &changes_attrs(c.changes_only))?;
        x.text("Ид", &c.id)?;
        x.opt("ИдКлассификатора", c.classifier_id.as_deref())?;
        x.text("Наименование", &c.name)?;
        if let Some(o) = &c.owner {
            counterparty(x, "Владелец", o)?;
        }
        x.start("Товары", &[])?;
        let tail = Catalog {
            description: c.description.clone(),
            signatories: c.signatories.clone(),
            extra: c.extra.clone(),
            ..Catalog::default()
        };
        self.state = State::Catalog(Box::new(tail));
        Ok(())
    }

    /// Writes a `Товар` inside an open catalog.
    pub fn product(&mut self, p: &Product) -> Result<()> {
        if !matches!(self.state, State::Catalog(_)) {
            return Err(Error::WriterState("product() outside of a catalog"));
        }
        Ok(product(&mut self.xml, p)?)
    }

    /// Closes `Каталог`.
    pub fn end_catalog(&mut self) -> Result<()> {
        let State::Catalog(tail) = std::mem::replace(&mut self.state, State::Root) else {
            return Err(Error::WriterState("end_catalog() without begin_catalog()"));
        };
        let x = &mut self.xml;
        x.end("Товары")?;
        x.opt("Описание", tail.description.as_deref())?;
        signatories(x, &tail.signatories)?;
        x.raw(&tail.extra)?;
        x.end("Каталог")?;
        Ok(())
    }

    /// Opens `ПакетПредложений` and writes its header; `offers` of `p` are ignored.
    pub fn begin_offer_package(&mut self, p: &OfferPackage) -> Result<()> {
        self.expect_root("begin_offer_package() inside an open section")?;
        let x = &mut self.xml;
        x.start("ПакетПредложений", &changes_attrs(p.changes_only))?;
        x.opt("Ид", p.id.as_deref())?;
        x.text("Наименование", &p.name)?;
        x.text("ИдКаталога", &p.catalog_id)?;
        x.opt("ИдКлассификатора", p.classifier_id.as_deref())?;
        x.opt("ДействительноС", p.valid_from.as_deref())?;
        x.opt("ДействительноДо", p.valid_to.as_deref())?;
        x.opt("Описание", p.description.as_deref())?;
        if let Some(o) = &p.owner {
            counterparty(x, "Владелец", o)?;
        }
        if p.price_types_classifier_id.is_some() || !p.price_types.is_empty() {
            x.start("ТипыЦен", &[])?;
            x.opt("ИдКлассификатора", p.price_types_classifier_id.as_deref())?;
            for t in &p.price_types {
                price_type(x, t)?;
            }
            x.end("ТипыЦен")?;
        }
        list(x, "Склады", &p.warehouses, warehouse)?;
        list(x, "ЗначенияСвойств", &p.property_values, property_value)?;
        x.start("Предложения", &[])?;
        let tail = OfferPackage {
            signatories: p.signatories.clone(),
            extra: p.extra.clone(),
            ..OfferPackage::default()
        };
        self.state = State::OfferPackage(Box::new(tail));
        Ok(())
    }

    /// Writes a `Предложение` inside an open offer package.
    pub fn offer(&mut self, o: &Offer) -> Result<()> {
        if !matches!(self.state, State::OfferPackage(_)) {
            return Err(Error::WriterState("offer() outside of an offer package"));
        }
        Ok(offer(&mut self.xml, o)?)
    }

    /// Closes `ПакетПредложений`.
    pub fn end_offer_package(&mut self) -> Result<()> {
        let State::OfferPackage(tail) = std::mem::replace(&mut self.state, State::Root) else {
            return Err(Error::WriterState(
                "end_offer_package() without begin_offer_package()",
            ));
        };
        let x = &mut self.xml;
        x.end("Предложения")?;
        signatories(x, &tail.signatories)?;
        x.raw(&tail.extra)?;
        x.end("ПакетПредложений")?;
        Ok(())
    }

    /// Opens `ИзмененияПакетаПредложений`; `offers` of `c` are ignored.
    pub fn begin_offer_package_changes(&mut self, c: &OfferPackageChanges) -> Result<()> {
        self.expect_root("begin_offer_package_changes() inside an open section")?;
        let x = &mut self.xml;
        x.start("ИзмененияПакетаПредложений", &changes_attrs(c.changes_only))?;
        x.text("Ид", &c.id)?;
        x.opt("ИдКаталога", c.catalog_id.as_deref())?;
        x.start("Предложения", &[])?;
        let tail = OfferPackageChanges {
            extra: c.extra.clone(),
            ..OfferPackageChanges::default()
        };
        self.state = State::Changes(Box::new(tail));
        Ok(())
    }

    /// Writes a changed `Предложение`.
    pub fn offer_change(&mut self, o: &OfferChange) -> Result<()> {
        if !matches!(self.state, State::Changes(_)) {
            return Err(Error::WriterState(
                "offer_change() outside of offer package changes",
            ));
        }
        Ok(offer_change(&mut self.xml, o)?)
    }

    /// Closes `ИзмененияПакетаПредложений`.
    pub fn end_offer_package_changes(&mut self) -> Result<()> {
        let State::Changes(tail) = std::mem::replace(&mut self.state, State::Root) else {
            return Err(Error::WriterState(
                "end_offer_package_changes() without begin_offer_package_changes()",
            ));
        };
        let x = &mut self.xml;
        x.end("Предложения")?;
        x.raw(&tail.extra)?;
        x.end("ИзмененияПакетаПредложений")?;
        Ok(())
    }

    /// Writes a `Документ`.
    pub fn document(&mut self, d: &Document) -> Result<()> {
        self.expect_root("document() inside an open section")?;
        Ok(document(&mut self.xml, "Документ", d)?)
    }

    /// Opens a root-level `Контрагенты` list.
    pub fn begin_counterparties(&mut self) -> Result<()> {
        self.expect_root("begin_counterparties() inside an open section")?;
        self.xml.start("Контрагенты", &[])?;
        self.state = State::Counterparties;
        Ok(())
    }

    /// Writes a `Контрагент` inside an open `Контрагенты` list.
    pub fn counterparty(&mut self, c: &Counterparty) -> Result<()> {
        if !matches!(self.state, State::Counterparties) {
            return Err(Error::WriterState(
                "counterparty() outside of begin_counterparties()",
            ));
        }
        Ok(counterparty(&mut self.xml, "Контрагент", c)?)
    }

    /// Closes the `Контрагенты` list.
    pub fn end_counterparties(&mut self) -> Result<()> {
        if !matches!(self.state, State::Counterparties) {
            return Err(Error::WriterState(
                "end_counterparties() without begin_counterparties()",
            ));
        }
        self.state = State::Root;
        Ok(self.xml.end("Контрагенты")?)
    }

    /// Writes kept unknown root-level elements verbatim.
    pub fn raw(&mut self, extra: &[RawElement]) -> Result<()> {
        self.expect_root("raw() inside an open section")?;
        Ok(self.xml.raw(extra)?)
    }

    /// Closes the root element, flushes and returns the output.
    pub fn finish(mut self) -> Result<T> {
        self.expect_root("finish() with an open section")?;
        self.xml.end("КоммерческаяИнформация")?;
        Ok(self.xml.finish()?)
    }
}

/// Serializes a whole document with the given options.
pub(crate) fn write_info<T: Write>(info: &CommercialInfo, out: T, options: &WriteOptions) -> Result<T> {
    let header = Header {
        schema_version: info.schema_version.clone(),
        generated_at: info.generated_at.clone(),
    };
    let mut w = Writer::new(out, &header, options)?;
    if let Some(c) = &info.classifier {
        w.classifier(c)?;
    }
    if let Some(c) = &info.catalog {
        w.begin_catalog(c)?;
        for p in &c.products {
            w.product(p)?;
        }
        w.end_catalog()?;
    }
    if let Some(p) = &info.offer_package {
        w.begin_offer_package(p)?;
        for o in &p.offers {
            w.offer(o)?;
        }
        w.end_offer_package()?;
    }
    for c in &info.offer_package_changes {
        w.begin_offer_package_changes(c)?;
        for o in &c.offers {
            w.offer_change(o)?;
        }
        w.end_offer_package_changes()?;
    }
    for d in &info.documents {
        w.document(d)?;
    }
    if !info.counterparties.is_empty() {
        w.begin_counterparties()?;
        for c in &info.counterparties {
            w.counterparty(c)?;
        }
        w.end_counterparties()?;
    }
    w.raw(&info.extra)?;
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_rejects_misuse() {
        let h = Header::default();
        let mut w = Writer::new(Vec::new(), &h, &WriteOptions::default()).unwrap();
        assert!(matches!(
            w.product(&Product::default()),
            Err(Error::WriterState(_))
        ));
        assert!(w.end_catalog().is_err());
        assert!(w.end_offer_package().is_err());
        assert!(w.end_offer_package_changes().is_err());
        assert!(w.end_counterparties().is_err());
        assert!(w.offer(&Offer::default()).is_err());
        assert!(w.offer_change(&OfferChange::default()).is_err());
        assert!(w.counterparty(&Counterparty::default()).is_err());
        w.begin_catalog(&Catalog::default()).unwrap();
        assert!(w.document(&Document::default()).is_err());
        assert!(w.classifier(&Classifier::default()).is_err());
        assert!(w.begin_offer_package(&OfferPackage::default()).is_err());
        assert!(
            w.begin_offer_package_changes(&OfferPackageChanges::default())
                .is_err()
        );
        assert!(w.begin_counterparties().is_err());
        assert!(w.raw(&[]).is_err());
        assert!(w.begin_catalog(&Catalog::default()).is_err());
        let w = {
            let mut w = w;
            w.end_catalog().unwrap();
            w
        };
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert!(out.contains("<Товары>"));
    }

    #[test]
    fn unfinished_section_is_an_error() {
        let mut w = Writer::new(Vec::new(), &Header::default(), &WriteOptions::default()).unwrap();
        w.begin_counterparties().unwrap();
        assert!(w.finish().is_err());
    }
}
