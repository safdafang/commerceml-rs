use std::fmt;

/// Declares a string enumeration from the standard with an `Other` variant
/// for values outside it. `parse` also accepts listed aliases.
macro_rules! cml_enum {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $text:literal $(| $alias:literal)* ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value not listed in the standard, kept verbatim.
            Other(String),
        }

        impl $name {
            /// The text used in XML.
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $text, )+
                    Self::Other(s) => s,
                }
            }

            /// Maps XML text to a variant; unknown text becomes [`Self::Other`].
            pub fn parse(text: &str) -> Self {
                match text.trim() {
                    $( $text $(| $alias)* => Self::$variant, )+
                    _ => Self::Other(text.to_owned()),
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl From<&str> for $name {
            fn from(text: &str) -> Self {
                Self::parse(text)
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <String as serde::Deserialize>::deserialize(d)?;
                Ok(Self::parse(&s))
            }
        }
    };
}

cml_enum! {
    /// Role of a party in a document (`РольТип`).
    Role {
        /// `Продавец`
        Seller = "Продавец",
        /// `Покупатель`
        Buyer = "Покупатель",
        /// `Плательщик`
        Payer = "Плательщик",
        /// `Получатель`
        Recipient = "Получатель",
        /// `Комитент`
        Consignor = "Комитент",
        /// `Комиссионер`
        CommissionAgent = "Комиссионер",
        /// `Лицензиар`
        Licensor = "Лицензиар",
        /// `Лицензиат` (also spelt `Лицензиант` in the 2.10 change list)
        Licensee = "Лицензиат" | "Лицензиант",
    }
}

cml_enum! {
    /// Business operation of a document (`ХозОперацияТип`).
    BusinessOperation {
        /// `Заказ товара` — an order.
        OrderGoods = "Заказ товара" | "ЗаказТовара",
        /// `Счет на оплату` — an invoice for payment.
        PaymentInvoice = "Счет на оплату" | "СчетНаОплату" | "CчетНаОплату",
        /// `Отпуск товара` — shipment.
        ReleaseGoods = "Отпуск товара" | "ОтпускТовара",
        /// `Счет-фактура` — VAT invoice.
        TaxInvoice = "Счет-фактура" | "СчетФактура",
        /// `Возврат товара` — return.
        ReturnGoods = "Возврат товара" | "ВозвратТовара",
        /// `Передача товара на комиссию`
        TransferToCommission = "Передача товара на комиссию" | "ПередачаТовараНаКомиссию",
        /// `Возврат комиссионного товара`
        ReturnCommissionGoods = "Возврат комиссионного товара" | "ВозвратКомиссионногоТовара",
        /// `Отчет о продажах комиссионного товара`
        CommissionSalesReport = "Отчет о продажах комиссионного товара" | "ОтчетОПродажахКомиссионногоТовара",
        /// `Отчет о списании комиссионного товара` (since 2.07)
        CommissionWriteOffReport = "Отчет о списании комиссионного товара",
        /// `Выплата наличных денег`
        CashPayment = "Выплата наличных денег" | "ВыплатаНаличныхДенег",
        /// `Возврат наличных денег`
        CashRefund = "Возврат наличных денег" | "ВозвратНаличныхДенег",
        /// `Выплата безналичных денег`
        BankPayment = "Выплата безналичных денег" | "ВыплатаБезналичныхДенег",
        /// `Возврат безналичных денег`
        BankRefund = "Возврат безналичных денег" | "ВозвратБезналичныхДенег",
        /// `Переоценка товаров`
        Revaluation = "Переоценка товаров",
        /// `Передача прав`
        RightsTransfer = "Передача прав",
        /// `Прочие`
        Misc = "Прочие" | "Прочее",
    }
}

cml_enum! {
    /// Status of a product in an incremental export (`СтатусТип`, attribute `Статус`).
    ProductStatus {
        /// `Новый`
        New = "Новый",
        /// `Изменен`
        Changed = "Изменен",
        /// `Удален`
        Deleted = "Удален",
    }
}

cml_enum! {
    /// Type of property values (`ТипЗначенийТип`).
    PropertyValueType {
        /// `Строка`
        String = "Строка",
        /// `Число`
        Number = "Число",
        /// `Время`
        Time = "Время",
        /// `Справочник` — values come from [`Property::dictionary`](crate::Property::dictionary).
        Dictionary = "Справочник",
    }
}

cml_enum! {
    /// Where a property is mandatory (`ОбязательностьСвойствТип`).
    RequiredFor {
        /// `Для каталога`
        Catalog = "Для каталога",
        /// `Для документа`
        Document = "Для документа",
        /// `Для предложений`
        Offers = "Для предложений",
    }
}

cml_enum! {
    /// Kind of contact (`КонтактТип`).
    ContactType {
        /// `Телефон внутренний`
        PhoneInternal = "Телефон внутренний",
        /// `Телефон рабочий`
        PhoneWork = "Телефон рабочий",
        /// `Телефон мобильный`
        PhoneMobile = "Телефон мобильный",
        /// `Телефон домашний`
        PhoneHome = "Телефон домашний",
        /// `Пейджер`
        Pager = "Пейджер",
        /// `Факс`
        Fax = "Факс",
        /// `Почта` (Bitrix uses `Электронная почта`)
        Email = "Почта" | "Электронная почта",
        /// `ICQ`
        Icq = "ICQ",
        /// `ВебСайт`
        Website = "ВебСайт",
        /// `Координаты на карте` (since 2.07)
        MapCoordinates = "Координаты на карте",
        /// `Прочее` (since 2.07)
        Misc = "Прочее",
    }
}

cml_enum! {
    /// Kind of address part (`АдресТип`).
    AddressFieldType {
        /// `Почтовый индекс`
        PostalCode = "Почтовый индекс",
        /// `Страна`
        Country = "Страна",
        /// `Регион`
        Region = "Регион",
        /// `Район`
        District = "Район",
        /// `Населенный пункт`
        Locality = "Населенный пункт",
        /// `Город`
        City = "Город",
        /// `Улица`
        Street = "Улица",
        /// `Дом`
        House = "Дом",
        /// `Корпус`
        Building = "Корпус",
        /// `Квартира`
        Apartment = "Квартира",
    }
}

cml_enum! {
    /// Gender (`ПолТип`).
    Gender {
        /// `М`
        Male = "М",
        /// `Ж`
        Female = "Ж",
    }
}

/// An element the library does not model, kept verbatim.
///
/// Unknown elements found inside major entities (products, offers,
/// documents, groups, …) are preserved here and written back unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawElement {
    /// Local name of the element.
    pub name: String,
    /// The complete element as UTF-8 XML, e.g. `<Бренд>Acme</Бренд>`.
    pub xml: String,
}

impl fmt::Display for RawElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.xml)
    }
}

/// A named value: `ЗначениеРеквизита` (`Наименование` + `Значение`).
///
/// 1C uses these for data without a dedicated element, e.g.
/// `ВидНоменклатуры`, `Полное наименование`, `Статус заказа`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Requisite {
    /// `Наименование`.
    pub name: String,
    /// `Значение`, one or more.
    pub values: Vec<String>,
}

impl Requisite {
    /// A requisite with a single value.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: vec![value.into()],
        }
    }

    /// The first value, if any.
    pub fn value(&self) -> Option<&str> {
        self.values.first().map(String::as_str)
    }
}

/// Lookup helpers for lists of [`Requisite`].
pub trait RequisitesExt {
    /// The first value of the requisite with this name.
    fn requisite(&self, name: &str) -> Option<&str>;
}

impl RequisitesExt for [Requisite] {
    fn requisite(&self, name: &str) -> Option<&str> {
        self.iter().find(|r| r.name == name).and_then(Requisite::value)
    }
}

/// One part of a structured address: `АдресноеПоле`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AddressField {
    /// `Тип`.
    pub kind: AddressFieldType,
    /// `Значение`.
    pub value: String,
}

/// An address: `Адрес` and the elements of the same type
/// (`ЮридическийАдрес`, `АдресРегистрации`, `МестоРождения`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Address {
    /// `Представление` — the address as one line.
    pub presentation: String,
    /// `Комментарий`.
    pub comment: Option<String>,
    /// `АдресноеПоле`, at most 10 by the standard.
    pub fields: Vec<AddressField>,
}

/// A contact: `Контакт` (phone, e-mail…).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Contact {
    /// `Тип`.
    pub kind: ContactType,
    /// `Значение`.
    pub value: String,
    /// `Комментарий`.
    pub comment: Option<String>,
}

/// A tax as declared for a price type: `Налог` (`Наименование`,
/// `УчтеноВСумме`, `Акциз`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tax {
    /// `Наименование`, e.g. `НДС`.
    pub name: String,
    /// `УчтеноВСумме` — whether prices include this tax.
    pub included_in_amount: Option<bool>,
    /// `Акциз` — whether the tax is an excise.
    pub excise: Option<bool>,
}

/// A warehouse: `Склад`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Warehouse {
    /// `Ид`.
    pub id: String,
    /// `Наименование`.
    pub name: Option<String>,
    /// `Комментарий`.
    pub comment: Option<String>,
    /// `Адрес`.
    pub address: Option<crate::Address>,
    /// `Контакты`.
    pub contacts: Vec<Contact>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_parse_and_aliases() {
        assert_eq!(
            BusinessOperation::parse("Заказ товара"),
            BusinessOperation::OrderGoods
        );
        assert_eq!(
            BusinessOperation::parse("ЗаказТовара"),
            BusinessOperation::OrderGoods
        );
        assert_eq!(BusinessOperation::OrderGoods.as_str(), "Заказ товара");
        assert_eq!(
            BusinessOperation::parse("Возврат подарка"),
            BusinessOperation::Other("Возврат подарка".into())
        );
        assert_eq!(BusinessOperation::from("Прочее").to_string(), "Прочие");
        assert_eq!(ContactType::parse("Электронная почта"), ContactType::Email);
        assert_eq!(Role::parse(" Покупатель "), Role::Buyer);
        assert_eq!(Gender::Female.as_str(), "Ж");
    }

    #[test]
    fn requisite_lookup() {
        let list = [Requisite::new("ВидНоменклатуры", "Товар"), Requisite::default()];
        assert_eq!(list.requisite("ВидНоменклатуры"), Some("Товар"));
        assert_eq!(list.requisite("Нет"), None);
        assert_eq!(list[1].value(), None);
    }
}
