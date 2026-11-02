//! Readers for individual entities.
//!
//! `fn x(r, e) -> Result<X>` reads a whole element. `fn x_field(r, c, &mut
//! X) -> Result<Option<Elem>>` handles one child element and hands it back
//! when it is not one of `X`'s fields, so that extended types (an offer is a
//! product plus prices) can reuse the base fields.

use std::io::BufRead;

use crate::Decimal;
use crate::error::{Result, WarningKind};
use crate::model::{
    Address, AddressField, AddressFieldType, Adjustment, Bank, BankAccount, BaseUnit, BusinessOperation,
    Catalog, Category, Characteristic, Classifier, Contact, ContactType, Counterparty, DictionaryValue,
    Document, DocumentItem, DocumentTax, Excise, Gender, Group, Head, IdentityDocument, Offer, OfferChange,
    OfferPackage, OfferPackageChanges, Price, PriceType, Product, ProductStatus, Property, PropertyValue,
    PropertyValueType, RelatedProduct, Representative, RequiredFor, Requisite, Rest, Role, Signatory,
    SitePayment, Tax, TaxRate, Unit, UnitConversion, Warehouse, WarehouseQuantity, WarehouseStock,
};
use crate::xml::reader::{Elem, XmlReader};

type Rd<B> = XmlReader<B>;

fn text<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<String> {
    r.text(e)
}

fn opt<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Option<String>> {
    r.text(e).map(Some)
}

pub(crate) fn parse_decimal<B: BufRead>(
    r: &mut Rd<B>,
    name: &str,
    value: &str,
    pos: u64,
    child: Option<&str>,
) -> Result<Option<Decimal>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    if let Ok(d) = Decimal::parse_lenient(value) {
        return Ok(Some(d));
    }
    r.warn(
        WarningKind::InvalidValue {
            name: name.to_owned(),
            value: value.to_owned(),
        },
        pos,
        child,
    )?;
    Ok(None)
}

fn decimal<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Option<Decimal>> {
    let t = r.text(e)?;
    parse_decimal(r, &e.name, &t, e.pos, Some(&e.name))
}

pub(crate) fn parse_bool<B: BufRead>(
    r: &mut Rd<B>,
    name: &str,
    value: &str,
    pos: u64,
) -> Result<Option<bool>> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("true") || v == "1" {
        Ok(Some(true))
    } else if v.eq_ignore_ascii_case("false") || v == "0" {
        Ok(Some(false))
    } else if v.is_empty() {
        Ok(None)
    } else {
        r.warn(
            WarningKind::InvalidValue {
                name: name.to_owned(),
                value: value.to_owned(),
            },
            pos,
            Some(name),
        )?;
        Ok(None)
    }
}

fn boolean<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Option<bool>> {
    let t = r.text(e)?;
    parse_bool(r, &e.name, &t, e.pos)
}

/// Reads a wrapper element holding a list of `item` elements.
fn list<B: BufRead, T>(
    r: &mut Rd<B>,
    e: &Elem,
    item: &str,
    mut f: impl FnMut(&mut Rd<B>, &Elem) -> Result<T>,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    while let Some(c) = r.next_child(e)? {
        if c.name == item {
            out.push(f(r, &c)?);
        } else {
            r.unknown(c, None)?;
        }
    }
    Ok(out)
}

/// Reads an element whose children are all handled by `field`.
fn object<B: BufRead, T: Default>(
    r: &mut Rd<B>,
    e: &Elem,
    mut field: impl FnMut(&mut Rd<B>, Elem, &mut T) -> Result<Option<Elem>>,
) -> Result<T> {
    let mut v = T::default();
    while let Some(c) = r.next_child(e)? {
        if let Some(c) = field(r, c, &mut v)? {
            r.unknown(c, None)?;
        }
    }
    Ok(v)
}

pub(crate) fn address<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Address> {
    object(r, e, |r, c, a: &mut Address| {
        match c.name.as_str() {
            "Представление" => a.presentation = text(r, &c)?,
            "Комментарий" => a.comment = opt(r, &c)?,
            "АдресноеПоле" => {
                let (mut kind, mut value) = (None, String::new());
                while let Some(f) = r.next_child(&c)? {
                    match f.name.as_str() {
                        "Тип" => kind = Some(AddressFieldType::parse(&text(r, &f)?)),
                        "Значение" => value = text(r, &f)?,
                        _ => r.unknown(f, None)?,
                    }
                }
                a.fields.push(AddressField {
                    kind: kind.unwrap_or_else(|| AddressFieldType::Other(String::new())),
                    value,
                });
            }
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn contacts<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Vec<Contact>> {
    list(r, e, "Контакт", |r, c| {
        let mut contact = Contact {
            kind: ContactType::Other(String::new()),
            value: String::new(),
            comment: None,
        };
        while let Some(f) = r.next_child(c)? {
            match f.name.as_str() {
                "Тип" => contact.kind = ContactType::parse(&text(r, &f)?),
                "Значение" => contact.value = text(r, &f)?,
                "Комментарий" => contact.comment = opt(r, &f)?,
                _ => r.unknown(f, None)?,
            }
        }
        Ok(contact)
    })
}

pub(crate) fn requisite<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Requisite> {
    object(r, e, |r, c, q: &mut Requisite| {
        match c.name.as_str() {
            "Наименование" => q.name = text(r, &c)?,
            "Значение" => q.values.push(text(r, &c)?),
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn requisites<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Vec<Requisite>> {
    list(r, e, "ЗначениеРеквизита", requisite)
}

fn tax_field<B: BufRead>(r: &mut Rd<B>, c: Elem, t: &mut Tax) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Наименование" => t.name = text(r, &c)?,
        "УчтеноВСумме" => t.included_in_amount = boolean(r, &c)?,
        "Акциз" => t.excise = boolean(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn tax<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Tax> {
    object(r, e, tax_field)
}

fn warehouse_field<B: BufRead>(r: &mut Rd<B>, c: Elem, w: &mut Warehouse) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => w.id = text(r, &c)?,
        "Наименование" => w.name = opt(r, &c)?,
        "Комментарий" => w.comment = opt(r, &c)?,
        "Адрес" => w.address = Some(address(r, &c)?),
        "Контакты" => w.contacts = contacts(r, &c)?,
        "ПометкаУдаления" => w.deletion_mark = boolean(r, &c)?,
        "НомерВерсии" => w.version = opt(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn warehouse<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Warehouse> {
    let mut w = Warehouse::default();
    while let Some(c) = r.next_child(e)? {
        if let Some(c) = warehouse_field(r, c, &mut w)? {
            r.unknown(c, Some(&mut w.extra))?;
        }
    }
    Ok(w)
}

fn identity_document<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<IdentityDocument> {
    object(r, e, |r, c, d: &mut IdentityDocument| {
        match c.name.as_str() {
            "ВидДокумента" => d.kind = text(r, &c)?,
            "Серия" => d.series = opt(r, &c)?,
            "Номер" => d.number = text(r, &c)?,
            "ДатаВыдачи" => d.issue_date = text(r, &c)?,
            "КемВыдан" => d.issued_by = text(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

fn bank<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Bank> {
    object(r, e, |r, c, b: &mut Bank| {
        match c.name.as_str() {
            "СчетКорреспондентский" => b.correspondent_account = opt(r, &c)?,
            "Наименование" => b.name = opt(r, &c)?,
            "Адрес" => b.address = Some(address(r, &c)?),
            "Контакты" => b.contacts = contacts(r, &c)?,
            "БИК" => b.bic = opt(r, &c)?,
            "SWIFT" => b.swift = opt(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

fn bank_account<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<BankAccount> {
    object(r, e, |r, c, a: &mut BankAccount| {
        match c.name.as_str() {
            "НомерСчета" => a.number = text(r, &c)?,
            "Банк" => a.bank = bank(r, &c)?,
            "БанкКорреспондент" => a.correspondent_bank = Some(bank(r, &c)?),
            "Комментарий" => a.comment = opt(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

fn head<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Head> {
    object(r, e, |r, c, h: &mut Head| {
        match c.name.as_str() {
            "Фамилия" => h.last_name = text(r, &c)?,
            "Имя" => h.first_name = text(r, &c)?,
            "Отчество" => h.middle_name = opt(r, &c)?,
            "УдостоверениеЛичности" => {
                h.identity_document = Some(identity_document(r, &c)?);
            }
            "АресРегистрации" | "АдресРегистрации" => {
                h.registration_address = Some(address(r, &c)?);
            }
            "Должность" => h.position = text(r, &c)?,
            "Контакты" => h.contacts = contacts(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn signatory<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Signatory> {
    object(r, e, |r, c, s: &mut Signatory| {
        match c.name.as_str() {
            "Фамилия" => s.last_name = text(r, &c)?,
            "Имя" => s.first_name = text(r, &c)?,
            "Отчество" => s.middle_name = opt(r, &c)?,
            "Обращение" => s.salutation = opt(r, &c)?,
            "УдостоверениеЛичности" => {
                s.identity_document = Some(identity_document(r, &c)?);
            }
            "АдресРегистрации" => s.registration_address = Some(address(r, &c)?),
            "МестоРаботы" => s.workplace = Some(Box::new(counterparty(r, &c)?)),
            "Должность" => s.position = opt(r, &c)?,
            "Комментарий" => s.comment = opt(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn signatories<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Vec<Signatory>> {
    list(r, e, "Подписант", signatory)
}

fn counterparty_field<B: BufRead>(r: &mut Rd<B>, c: Elem, p: &mut Counterparty) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => p.id = opt(r, &c)?,
        "Наименование" => p.name = opt(r, &c)?,
        "ОфициальноеНаименование" => p.official_name = opt(r, &c)?,
        "ЮридическийАдрес" => p.legal_address = Some(address(r, &c)?),
        "ИНН" => p.inn = opt(r, &c)?,
        "КПП" => p.kpp = opt(r, &c)?,
        "ОсновнойВидДеятельности" => p.main_activity = opt(r, &c)?,
        "ЕГРПО" => p.egrpo = opt(r, &c)?,
        "ОКВЭД" => p.okved = opt(r, &c)?,
        "ОКДП" => p.okdp = opt(r, &c)?,
        "ОКОПФ" => p.okopf = opt(r, &c)?,
        "ОКФС" => p.okfs = opt(r, &c)?,
        "ОКПО" | "КодПоОКПО" => p.okpo = opt(r, &c)?,
        "ДатаРегистрации" => p.registration_date = opt(r, &c)?,
        "Руководитель" => p.head = Some(head(r, &c)?),
        "РасчетныеСчета" => {
            p.bank_accounts = list(r, &c, "РасчетныйСчет", bank_account)?;
        }
        "ПолноеНаименование" => p.full_name = opt(r, &c)?,
        "Обращение" => p.salutation = opt(r, &c)?,
        "Фамилия" => p.last_name = opt(r, &c)?,
        "Имя" => p.first_name = opt(r, &c)?,
        "Отчество" => p.middle_name = opt(r, &c)?,
        "ДатаРождения" => p.birth_date = opt(r, &c)?,
        "МестоРождения" => p.birth_place = Some(address(r, &c)?),
        "Пол" => p.gender = Some(Gender::parse(&text(r, &c)?)),
        "УдостоверениеЛичности" => {
            p.identity_document = Some(identity_document(r, &c)?);
        }
        "АдресРегистрации" => p.registration_address = Some(address(r, &c)?),
        "Организация" => p.organization = Some(Box::new(counterparty(r, &c)?)),
        "Должность" => p.position = opt(r, &c)?,
        "Комментарий" => p.comment = opt(r, &c)?,
        "Адрес" => p.address = Some(address(r, &c)?),
        "Контакты" => p.contacts = contacts(r, &c)?,
        "Представители" => {
            p.representatives = list(r, &c, "Представитель", representative)?;
        }
        "Роль" => p.role = Some(Role::parse(&text(r, &c)?)),
        "РасчетныйСчет" => p.bank_account = Some(bank_account(r, &c)?),
        "Склад" => p.warehouse = Some(warehouse(r, &c)?),
        "ПометкаУдаления" => p.deletion_mark = boolean(r, &c)?,
        "НомерВерсии" => p.version = opt(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn counterparty<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Counterparty> {
    let mut p = Counterparty::default();
    while let Some(c) = r.next_child(e)? {
        if let Some(c) = counterparty_field(r, c, &mut p)? {
            r.unknown(c, Some(&mut p.extra))?;
        }
    }
    Ok(p)
}

fn representative<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Representative> {
    let mut rep = Representative::default();
    let field = |r: &mut Rd<B>, c: Elem, rep: &mut Representative| -> Result<()> {
        if c.name == "Отношение" {
            rep.relation = text(r, &c)?;
        } else if let Some(c) = counterparty_field(r, c, &mut rep.counterparty)? {
            r.unknown(c, Some(&mut rep.counterparty.extra))?;
        }
        Ok(())
    };
    while let Some(c) = r.next_child(e)? {
        if c.name == "Контрагент" {
            while let Some(cc) = r.next_child(&c)? {
                field(r, cc, &mut rep)?;
            }
        } else {
            field(r, c, &mut rep)?;
        }
    }
    Ok(rep)
}

pub(crate) fn classifier_field<B: BufRead>(
    r: &mut Rd<B>,
    c: Elem,
    k: &mut Classifier,
) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => k.id = text(r, &c)?,
        "Наименование" => k.name = text(r, &c)?,
        "Владелец" => k.owner = Some(counterparty(r, &c)?),
        "Описание" => k.description = opt(r, &c)?,
        "Подписанты" => k.signatories = signatories(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn group<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Group> {
    let mut g = Group::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => g.id = text(r, &c)?,
            "Наименование" => g.name = text(r, &c)?,
            "Описание" => g.description = opt(r, &c)?,
            "Свойства" => g.properties = list(r, &c, "Свойство", property)?,
            "Группы" => g.groups = list(r, &c, "Группа", group)?,
            "ПометкаУдаления" => g.deletion_mark = boolean(r, &c)?,
            "НомерВерсии" => g.version = opt(r, &c)?,
            _ => r.unknown(c, Some(&mut g.extra))?,
        }
    }
    Ok(g)
}

pub(crate) fn property<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Property> {
    let mut p = Property::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => p.id = text(r, &c)?,
            "Наименование" => p.name = text(r, &c)?,
            "Описание" => p.description = opt(r, &c)?,
            "Обязательное" => p.required_for = Some(RequiredFor::parse(&text(r, &c)?)),
            "Множественное" => p.multiple = boolean(r, &c)?,
            "ТипЗначений" => {
                p.value_type = Some(PropertyValueType::parse(&text(r, &c)?));
            }
            "ВариантыЗначений" => {
                while let Some(v) = r.next_child(&c)? {
                    match v.name.as_str() {
                        "Значение" => p.values.push(text(r, &v)?),
                        "Справочник" => {
                            let d = object(r, &v, |r, f, d: &mut DictionaryValue| {
                                match f.name.as_str() {
                                    "ИдЗначения" => d.value_id = opt(r, &f)?,
                                    "Значение" => d.value = text(r, &f)?,
                                    "Картинка" => d.picture = opt(r, &f)?,
                                    _ => return Ok(Some(f)),
                                }
                                Ok(None)
                            })?;
                            p.dictionary.push(d);
                        }
                        _ => r.unknown(v, None)?,
                    }
                }
            }
            "ДляТоваров" => p.for_products = boolean(r, &c)?,
            "ДляПредложений" => p.for_offers = boolean(r, &c)?,
            "ДляДокументов" => p.for_documents = boolean(r, &c)?,
            "Внешний" => p.external = boolean(r, &c)?,
            "ПометкаУдаления" => p.deletion_mark = boolean(r, &c)?,
            "НомерВерсии" => p.version = opt(r, &c)?,
            _ => r.unknown(c, Some(&mut p.extra))?,
        }
    }
    Ok(p)
}

pub(crate) fn price_type<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<PriceType> {
    let mut t = PriceType::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => t.id = text(r, &c)?,
            "Наименование" => t.name = opt(r, &c)?,
            "Валюта" => t.currency = opt(r, &c)?,
            "Описание" => t.description = opt(r, &c)?,
            "Налог" => t.taxes.push(tax(r, &c)?),
            "ПометкаУдаления" => t.deletion_mark = boolean(r, &c)?,
            "НомерВерсии" => t.version = opt(r, &c)?,
            _ => r.unknown(c, Some(&mut t.extra))?,
        }
    }
    Ok(t)
}

pub(crate) fn unit<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Unit> {
    let mut u = Unit::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => u.id = opt(r, &c)?,
            "Код" => u.code = opt(r, &c)?,
            "НаименованиеПолное" => u.full_name = opt(r, &c)?,
            "НаименованиеКраткое" => u.short_name = opt(r, &c)?,
            "МеждународноеСокращение" => u.international_code = opt(r, &c)?,
            "ПометкаУдаления" => u.deletion_mark = boolean(r, &c)?,
            _ => r.unknown(c, Some(&mut u.extra))?,
        }
    }
    Ok(u)
}

pub(crate) fn category<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Category> {
    let mut k = Category::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => k.id = text(r, &c)?,
            "Наименование" => k.name = text(r, &c)?,
            "Описание" => k.description = opt(r, &c)?,
            "ПометкаУдаления" => k.deletion_mark = boolean(r, &c)?,
            _ => r.unknown(c, Some(&mut k.extra))?,
        }
    }
    Ok(k)
}

pub(crate) fn catalog_field<B: BufRead>(r: &mut Rd<B>, c: Elem, k: &mut Catalog) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => k.id = text(r, &c)?,
        "ИдКлассификатора" => k.classifier_id = opt(r, &c)?,
        "Наименование" => k.name = text(r, &c)?,
        "Владелец" => k.owner = Some(counterparty(r, &c)?),
        "Описание" => k.description = opt(r, &c)?,
        "Подписанты" => k.signatories = signatories(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

fn unit_conversion_field<B: BufRead>(r: &mut Rd<B>, c: Elem, u: &mut UnitConversion) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Единица" => u.unit = opt(r, &c)?,
        "Коэффициент" => u.coefficient = opt(r, &c)?,
        "ДополнительныеДанные" => u.extra_data = requisites(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

fn base_unit<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<BaseUnit> {
    r.check_attrs(
        e,
        &[
            "Код",
            "НаименованиеПолное",
            "МеждународноеСокращение",
            "НаименованиеКраткое",
        ],
    )?;
    let mut u = BaseUnit {
        code: e.attr("Код").map(str::to_owned),
        full_name: e.attr("НаименованиеПолное").map(str::to_owned),
        international_code: e.attr("МеждународноеСокращение").map(str::to_owned),
        short_name: e.attr("НаименованиеКраткое").map(str::to_owned),
        ..BaseUnit::default()
    };
    let mut body = String::new();
    let mut conversions = Vec::new();
    r.mixed(e, &mut body, |r, c| {
        if c.name == "Пересчет" {
            conversions.push(object(r, &c, unit_conversion_field)?);
            Ok(())
        } else {
            r.unknown(c, None)
        }
    })?;
    let body = body.trim();
    u.text = (!body.is_empty()).then(|| body.to_owned());
    u.conversions = conversions;
    Ok(u)
}

fn property_value<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<PropertyValue> {
    object(r, e, |r, c, v: &mut PropertyValue| {
        match c.name.as_str() {
            "Ид" => v.id = text(r, &c)?,
            "Наименование" => v.name = opt(r, &c)?,
            "Значение" => v.values.push(text(r, &c)?),
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn property_values<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Vec<PropertyValue>> {
    list(r, e, "ЗначенияСвойства", property_value)
}

fn related<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<RelatedProduct> {
    let mut rel = RelatedProduct::default();
    rel.product.status = product_status(r, e, &["Статус"])?;
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "ИдКаталога" => rel.catalog_id = opt(r, &c)?,
            "ИдКлассификатора" => rel.classifier_id = opt(r, &c)?,
            "Количество" => rel.quantity = decimal(r, &c)?,
            _ => {
                if let Some(c) = product_field(r, c, &mut rel.product)? {
                    r.unknown(c, Some(&mut rel.product.extra))?;
                }
            }
        }
    }
    Ok(rel)
}

fn product_status<B: BufRead>(r: &mut Rd<B>, e: &Elem, known: &[&str]) -> Result<Option<ProductStatus>> {
    r.check_attrs(e, known)?;
    Ok(e.attr("Статус").map(ProductStatus::parse))
}

pub(crate) fn product_field<B: BufRead>(r: &mut Rd<B>, c: Elem, p: &mut Product) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => p.id = text(r, &c)?,
        "Штрихкод" => p.barcode = opt(r, &c)?,
        "Артикул" => p.article = opt(r, &c)?,
        "Наименование" => p.name = text(r, &c)?,
        "БазоваяЕдиница" => p.base_unit = Some(base_unit(r, &c)?),
        "ИдТовараУКонтрагента" => p.counterparty_product_id = opt(r, &c)?,
        "Группы" => p.group_ids = list(r, &c, "Ид", |r, e| text(r, e))?,
        "Категория" => p.category_id = opt(r, &c)?,
        "Описание" => p.description = opt(r, &c)?,
        "Картинка" => p.pictures.push(text(r, &c)?),
        "Страна" => p.country = opt(r, &c)?,
        "ТорговаяМарка" => p.trademark = opt(r, &c)?,
        "ВладелецТорговойМарки" => {
            p.trademark_owner = Some(Box::new(counterparty(r, &c)?));
        }
        "Изготовитель" => p.manufacturer = Some(Box::new(counterparty(r, &c)?)),
        "ЗначенияСвойств" => p.property_values = property_values(r, &c)?,
        "СтавкиНалогов" => {
            p.tax_rates = list(r, &c, "СтавкаНалога", |r, e| {
                object(r, e, |r, f, t: &mut TaxRate| {
                    match f.name.as_str() {
                        "Наименование" => t.name = text(r, &f)?,
                        "Ставка" => t.rate = text(r, &f)?,
                        _ => return Ok(Some(f)),
                    }
                    Ok(None)
                })
            })?;
        }
        "Акцизы" => {
            p.excises = list(r, &c, "Акциз", |r, e| {
                object(r, e, |r, f, x: &mut Excise| {
                    match f.name.as_str() {
                        "Наименование" => x.name = text(r, &f)?,
                        "СуммаЗаЕдиницу" => x.amount_per_unit = decimal(r, &f)?,
                        "Валюта" => x.currency = text(r, &f)?,
                        _ => return Ok(Some(f)),
                    }
                    Ok(None)
                })
            })?;
        }
        "Комплектующие" => p.components = list(r, &c, "Комплектующее", related)?,
        "Аналоги" => p.analogs = list(r, &c, "Аналог", related)?,
        "ХарактеристикиТовара" => {
            p.characteristics = list(r, &c, "ХарактеристикаТовара", |r, e| {
                object(r, e, |r, f, x: &mut Characteristic| {
                    match f.name.as_str() {
                        "Ид" => x.id = opt(r, &f)?,
                        "Наименование" => x.name = text(r, &f)?,
                        "Значение" => x.value = text(r, &f)?,
                        _ => return Ok(Some(f)),
                    }
                    Ok(None)
                })
            })?;
        }
        "ЗначенияРеквизитов" => p.requisites = requisites(r, &c)?,
        "ПометкаУдаления" => p.deletion_mark = boolean(r, &c)?,
        "НомерВерсии" => p.version = opt(r, &c)?,
        "Вес" => p.weight = decimal(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn product<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Product> {
    let mut p = Product {
        status: product_status(r, e, &["Статус"])?,
        ..Product::default()
    };
    while let Some(c) = r.next_child(e)? {
        if let Some(c) = product_field(r, c, &mut p)? {
            r.unknown(c, Some(&mut p.extra))?;
        }
    }
    Ok(p)
}

pub(crate) fn offer_package_field<B: BufRead>(
    r: &mut Rd<B>,
    c: Elem,
    k: &mut OfferPackage,
) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => k.id = opt(r, &c)?,
        "Наименование" => k.name = text(r, &c)?,
        "ИдКаталога" => k.catalog_id = text(r, &c)?,
        "ИдКлассификатора" => k.classifier_id = opt(r, &c)?,
        "ДействительноС" => k.valid_from = opt(r, &c)?,
        "ДействительноДо" => k.valid_to = opt(r, &c)?,
        "Описание" => k.description = opt(r, &c)?,
        "Владелец" => k.owner = Some(counterparty(r, &c)?),
        "ТипыЦен" => {
            while let Some(t) = r.next_child(&c)? {
                match t.name.as_str() {
                    "ИдКлассификатора" => k.price_types_classifier_id = opt(r, &t)?,
                    "ТипЦены" => k.price_types.push(price_type(r, &t)?),
                    _ => r.unknown(t, None)?,
                }
            }
        }
        "Склады" => k.warehouses = list(r, &c, "Склад", warehouse)?,
        "ЗначенияСвойств" => k.property_values = property_values(r, &c)?,
        "Подписанты" => k.signatories = signatories(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

fn price_field<B: BufRead>(r: &mut Rd<B>, c: Elem, p: &mut Price) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Представление" => p.presentation = opt(r, &c)?,
        "ИдТипаЦены" => p.price_type_id = text(r, &c)?,
        "ЦенаЗаЕдиницу" => p.price_per_unit = decimal(r, &c)?,
        "Валюта" => p.currency = opt(r, &c)?,
        "МинКоличество" => p.min_quantity = decimal(r, &c)?,
        "ИдКаталога" => p.catalog_id = opt(r, &c)?,
        _ => return unit_conversion_field(r, c, &mut p.unit),
    }
    Ok(None)
}

fn prices<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Vec<Price>> {
    list(r, e, "Цена", |r, e| object(r, e, price_field))
}

fn warehouse_stock_attrs<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<WarehouseStock> {
    r.check_attrs(e, &["ИдСклада", "КоличествоНаСкладе"])?;
    let quantity = match e.attr("КоличествоНаСкладе") {
        Some(q) => parse_decimal(r, "КоличествоНаСкладе", q, e.pos, None)?,
        None => None,
    };
    let stock = WarehouseStock {
        warehouse_id: e.attr("ИдСклада").unwrap_or_default().to_owned(),
        quantity,
    };
    r.skip(e)?;
    Ok(stock)
}

fn rest<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Rest> {
    object(r, e, |r, c, x: &mut Rest| {
        match c.name.as_str() {
            "Количество" => x.quantity = decimal(r, &c)?,
            "Склад" => {
                x.warehouse = Some(object(r, &c, |r, f, s: &mut WarehouseStock| {
                    match f.name.as_str() {
                        "Ид" => s.warehouse_id = text(r, &f)?,
                        "Количество" => s.quantity = decimal(r, &f)?,
                        _ => return Ok(Some(f)),
                    }
                    Ok(None)
                })?);
            }
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn offer<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Offer> {
    let mut o = Offer::default();
    o.product.status = product_status(r, e, &["Статус"])?;
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Цены" => o.prices = prices(r, &c)?,
            "Количество" => o.quantity = decimal(r, &c)?,
            "Склад" if c.attr("ИдСклада").is_some() || c.empty => {
                o.warehouse_stock.push(warehouse_stock_attrs(r, &c)?);
            }
            "Остатки" => o.rests = list(r, &c, "Остаток", rest)?,
            _ => {
                if let Some(c) = product_field(r, c, &mut o.product)? {
                    r.unknown(c, Some(&mut o.product.extra))?;
                }
            }
        }
    }
    Ok(o)
}

pub(crate) fn changes_field<B: BufRead>(
    r: &mut Rd<B>,
    c: Elem,
    k: &mut OfferPackageChanges,
) -> Result<Option<Elem>> {
    match c.name.as_str() {
        "Ид" => k.id = text(r, &c)?,
        "ИдКаталога" => k.catalog_id = opt(r, &c)?,
        _ => return Ok(Some(c)),
    }
    Ok(None)
}

pub(crate) fn offer_change<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<OfferChange> {
    let mut o = OfferChange::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => o.id = opt(r, &c)?,
            "Штрихкод" => o.barcode = opt(r, &c)?,
            "Артикул" => o.article = opt(r, &c)?,
            "ИдХарактеристики" => o.characteristic_id = opt(r, &c)?,
            "КодЕдиницыИзмерения" => o.unit_code = opt(r, &c)?,
            "Склады" => o.warehouse_stock.push(warehouse_stock_attrs(r, &c)?),
            "Цены" => o.prices = prices(r, &c)?,
            "Количество" => o.quantity = decimal(r, &c)?,
            _ => r.unknown(c, Some(&mut o.extra))?,
        }
    }
    Ok(o)
}

fn document_tax<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<DocumentTax> {
    object(r, e, |r, c, t: &mut DocumentTax| {
        match c.name.as_str() {
            "Сумма" => t.amount = decimal(r, &c)?,
            "Ставка" => t.rate = opt(r, &c)?,
            _ => return tax_field(r, c, &mut t.tax),
        }
        Ok(None)
    })
}

fn adjustment<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Adjustment> {
    object(r, e, |r, c, a: &mut Adjustment| {
        match c.name.as_str() {
            "Наименование" => a.name = text(r, &c)?,
            "Сумма" => a.amount = decimal(r, &c)?,
            "Процент" => a.percent = opt(r, &c)?,
            "УчтеноВСумме" => a.included_in_amount = boolean(r, &c)?,
            "Комментарий" => a.comment = opt(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

fn document_item<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<DocumentItem> {
    let mut i = DocumentItem::default();
    i.product.status = product_status(r, e, &["Статус"])?;
    while let Some(c) = r.next_child(e)? {
        let c = match c.name.as_str() {
            "ИдКаталога" => {
                i.catalog_id = opt(r, &c)?;
                continue;
            }
            "ИдКлассификатора" => {
                i.classifier_id = opt(r, &c)?;
                continue;
            }
            "ЦенаЗаЕдиницу" => {
                i.price_per_unit = decimal(r, &c)?;
                continue;
            }
            "Количество" => {
                i.quantity = decimal(r, &c)?;
                continue;
            }
            "Сумма" => {
                i.amount = decimal(r, &c)?;
                continue;
            }
            "СтранаПроисхождения" => {
                i.country_of_origin = opt(r, &c)?;
                continue;
            }
            "ГТД" => {
                i.customs_declaration = opt(r, &c)?;
                continue;
            }
            "Налоги" => {
                i.taxes = list(r, &c, "Налог", document_tax)?;
                continue;
            }
            "Скидки" => {
                i.discounts = list(r, &c, "Скидка", adjustment)?;
                continue;
            }
            "ДопРасходы" => {
                i.extra_charges = list(r, &c, "ДопРасход", adjustment)?;
                continue;
            }
            "ДополнительныеЗначенияРеквизитов" => {
                i.additional_requisites = requisites(r, &c)?;
                continue;
            }
            "Склады" => {
                i.warehouses = list(r, &c, "Склад", |r, e| {
                    let mut wq = WarehouseQuantity::default();
                    while let Some(f) = r.next_child(e)? {
                        if f.name == "Количество" {
                            wq.quantity = decimal(r, &f)?;
                        } else if let Some(f) = warehouse_field(r, f, &mut wq.warehouse)? {
                            r.unknown(f, Some(&mut wq.warehouse.extra))?;
                        }
                    }
                    Ok(wq)
                })?;
                continue;
            }
            _ => c,
        };
        let Some(c) = unit_conversion_field(r, c, &mut i.unit)? else {
            continue;
        };
        if let Some(c) = product_field(r, c, &mut i.product)? {
            r.unknown(c, Some(&mut i.product.extra))?;
        }
    }
    Ok(i)
}

fn site_payment<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<SitePayment> {
    object(r, e, |r, c, p: &mut SitePayment| {
        match c.name.as_str() {
            "НомерДокумента" => p.document_number = text(r, &c)?,
            "НомерТранзакции" => p.transaction_number = text(r, &c)?,
            "ДатаОплаты" => p.date = text(r, &c)?,
            "СуммаОплаты" => p.amount = decimal(r, &c)?,
            "СпособОплаты" => p.method = text(r, &c)?,
            "ИдСпособаОплаты" => p.method_id = opt(r, &c)?,
            _ => return Ok(Some(c)),
        }
        Ok(None)
    })
}

pub(crate) fn document<B: BufRead>(r: &mut Rd<B>, e: &Elem) -> Result<Document> {
    let mut d = Document::default();
    while let Some(c) = r.next_child(e)? {
        match c.name.as_str() {
            "Ид" => d.id = text(r, &c)?,
            "Номер" => d.number = text(r, &c)?,
            "Дата" => d.date = text(r, &c)?,
            "ХозОперация" => d.operation = Some(BusinessOperation::parse(&text(r, &c)?)),
            "Роль" => d.role = Some(Role::parse(&text(r, &c)?)),
            "Валюта" => d.currency = opt(r, &c)?,
            "Курс" => d.rate = opt(r, &c)?,
            "Сумма" => d.amount = decimal(r, &c)?,
            "Контрагенты" => d.counterparties = list(r, &c, "Контрагент", counterparty)?,
            "Время" => d.time = opt(r, &c)?,
            "СрокПлатежа" => d.payment_due = opt(r, &c)?,
            "Комментарий" => d.comment = opt(r, &c)?,
            "Налоги" => d.taxes = list(r, &c, "Налог", document_tax)?,
            "Скидки" => d.discounts = list(r, &c, "Скидка", adjustment)?,
            "ДопРасходы" => d.extra_charges = list(r, &c, "ДопРасход", adjustment)?,
            "Склады" => d.warehouses = list(r, &c, "Склад", warehouse)?,
            "Товары" => d.items = list(r, &c, "Товар", document_item)?,
            "ЗначенияРеквизитов" => d.requisites = requisites(r, &c)?,
            "Подписанты" => d.signatories = signatories(r, &c)?,
            "Оплаты" => d.payments = list(r, &c, "Оплата", site_payment)?,
            "ПодчиненныеДокументы" => {
                d.subordinate_documents = list(r, &c, "ПодчиненныйДокумент", document)?;
            }
            "Номер1С" => d.number_1c = opt(r, &c)?,
            "Дата1С" => d.date_1c = opt(r, &c)?,
            "Основание" => d.basis = opt(r, &c)?,
            "ПометкаУдаления" => d.deletion_mark = boolean(r, &c)?,
            "НомерВерсии" => d.version = opt(r, &c)?,
            _ => r.unknown(c, Some(&mut d.extra))?,
        }
    }
    Ok(d)
}
