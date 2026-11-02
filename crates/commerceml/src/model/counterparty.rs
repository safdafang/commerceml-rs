use super::common::{Address, Contact, Gender, RawElement, Role, Warehouse};

/// A counterparty: `Контрагент`, also used for `Владелец`, `Изготовитель`,
/// `ВладелецТорговойМарки` and the organization in `МестоРаботы`.
///
/// The standard has a choice between legal-entity details
/// (`РеквизитыЮрЛица`) and individual details (`РеквизитыФизЛица`). Real
/// exports mix them, so both sets live in one flat struct; use
/// [`is_legal_entity`](Self::is_legal_entity) to tell them apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Counterparty {
    /// `Ид`.
    pub id: Option<String>,
    /// `Наименование`.
    pub name: Option<String>,

    /// `ОфициальноеНаименование` (legal entity).
    pub official_name: Option<String>,
    /// `ЮридическийАдрес` (legal entity).
    pub legal_address: Option<Address>,
    /// `ИНН` — taxpayer number.
    pub inn: Option<String>,
    /// `КПП` — tax registration reason code.
    pub kpp: Option<String>,
    /// `ОсновнойВидДеятельности`.
    pub main_activity: Option<String>,
    /// `ЕГРПО`.
    pub egrpo: Option<String>,
    /// `ОКВЭД`.
    pub okved: Option<String>,
    /// `ОКДП`.
    pub okdp: Option<String>,
    /// `ОКОПФ`.
    pub okopf: Option<String>,
    /// `ОКФС`.
    pub okfs: Option<String>,
    /// `ОКПО` (Bitrix writes `КодПоОКПО`, read as the same field).
    pub okpo: Option<String>,
    /// `ДатаРегистрации`.
    pub registration_date: Option<String>,
    /// `Руководитель` — head of the organization.
    pub head: Option<Head>,
    /// `РасчетныеСчета/РасчетныйСчет`.
    pub bank_accounts: Vec<BankAccount>,

    /// `ПолноеНаименование` (individual).
    pub full_name: Option<String>,
    /// `Обращение` — form of address.
    pub salutation: Option<String>,
    /// `Фамилия`.
    pub last_name: Option<String>,
    /// `Имя`.
    pub first_name: Option<String>,
    /// `Отчество`.
    pub middle_name: Option<String>,
    /// `ДатаРождения`.
    pub birth_date: Option<String>,
    /// `МестоРождения`.
    pub birth_place: Option<Address>,
    /// `Пол`.
    pub gender: Option<Gender>,
    /// `УдостоверениеЛичности`.
    pub identity_document: Option<IdentityDocument>,
    /// `АдресРегистрации`.
    pub registration_address: Option<Address>,
    /// `Организация` from the `МестоРаботы` group.
    pub organization: Option<Box<Counterparty>>,
    /// `Должность` from the `МестоРаботы` group.
    pub position: Option<String>,

    /// `Комментарий`.
    pub comment: Option<String>,
    /// `Адрес`.
    pub address: Option<Address>,
    /// `Контакты/Контакт`.
    pub contacts: Vec<Contact>,
    /// `Представители/Представитель`.
    pub representatives: Vec<Representative>,

    /// `Роль` — set for counterparties inside a [`Document`](crate::Document).
    pub role: Option<Role>,
    /// `РасчетныйСчет` — the account used in a document.
    pub bank_account: Option<BankAccount>,
    /// `Склад` — the counterparty's warehouse in a document.
    pub warehouse: Option<Warehouse>,

    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Counterparty {
    /// Whether legal-entity details are present (`ОфициальноеНаименование`).
    pub fn is_legal_entity(&self) -> bool {
        self.official_name.is_some()
    }

    /// The best available display name: `Наименование`, then
    /// `ОфициальноеНаименование`, then `ПолноеНаименование`.
    pub fn display_name(&self) -> Option<&str> {
        self.name
            .as_deref()
            .or(self.official_name.as_deref())
            .or(self.full_name.as_deref())
    }
}

/// A representative or contact person: `Представитель`.
///
/// Orders published by 1C and Bitrix wrap the person in an extra
/// `Контрагент` element (`Представитель/Контрагент/…`); both forms are read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Representative {
    /// `Отношение`, e.g. `Контактное лицо`.
    pub relation: String,
    /// The person or organization.
    pub counterparty: Counterparty,
}

/// Head of an organization: `Руководитель`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Head {
    /// `Фамилия`.
    pub last_name: String,
    /// `Имя`.
    pub first_name: String,
    /// `Отчество`.
    pub middle_name: Option<String>,
    /// `УдостоверениеЛичности`.
    pub identity_document: Option<IdentityDocument>,
    /// `АресРегистрации` (sic, as spelt in the XSD; `АдресРегистрации` is
    /// read too).
    pub registration_address: Option<Address>,
    /// `Должность`.
    pub position: String,
    /// `Контакты/Контакт`.
    pub contacts: Vec<Contact>,
}

/// An identity document: `УдостоверениеЛичности`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IdentityDocument {
    /// `ВидДокумента`.
    pub kind: String,
    /// `Серия`.
    pub series: Option<String>,
    /// `Номер`.
    pub number: String,
    /// `ДатаВыдачи`.
    pub issue_date: String,
    /// `КемВыдан`.
    pub issued_by: String,
}

/// A bank account: `РасчетныйСчет`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BankAccount {
    /// `НомерСчета`.
    pub number: String,
    /// `Банк`.
    pub bank: Bank,
    /// `БанкКорреспондент`.
    pub correspondent_bank: Option<Bank>,
    /// `Комментарий`.
    pub comment: Option<String>,
}

/// A bank: `Банк`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bank {
    /// `СчетКорреспондентский`.
    pub correspondent_account: Option<String>,
    /// `Наименование`.
    pub name: Option<String>,
    /// `Адрес`.
    pub address: Option<Address>,
    /// `Контакты/Контакт`.
    pub contacts: Vec<Contact>,
    /// `БИК`.
    pub bic: Option<String>,
    /// `SWIFT`.
    pub swift: Option<String>,
}

/// A person who signs a document or catalog: `Подписант`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Signatory {
    /// `Фамилия`.
    pub last_name: String,
    /// `Имя`.
    pub first_name: String,
    /// `Отчество`.
    pub middle_name: Option<String>,
    /// `Обращение`.
    pub salutation: Option<String>,
    /// `УдостоверениеЛичности`.
    pub identity_document: Option<IdentityDocument>,
    /// `АдресРегистрации`.
    pub registration_address: Option<Address>,
    /// `МестоРаботы` — the employer's legal details.
    pub workplace: Option<Box<Counterparty>>,
    /// `Должность`.
    pub position: Option<String>,
    /// `Комментарий`.
    pub comment: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_falls_back() {
        let mut c = Counterparty {
            full_name: Some("Иванов".into()),
            ..Counterparty::default()
        };
        assert_eq!(c.display_name(), Some("Иванов"));
        assert!(!c.is_legal_entity());
        c.official_name = Some("ООО".into());
        assert_eq!(c.display_name(), Some("ООО"));
        assert!(c.is_legal_entity());
        c.name = Some("Краткое".into());
        assert_eq!(c.display_name(), Some("Краткое"));
    }
}
