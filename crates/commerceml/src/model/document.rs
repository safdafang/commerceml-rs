use super::catalog::{Product, UnitConversion};
use super::common::{BusinessOperation, RawElement, Requisite, Role, Tax, Warehouse};
use super::counterparty::{Counterparty, Signatory};
use crate::Decimal;

/// A business document: `Документ` — most often an order (`Заказ товара`).
///
/// The same type is used for `ПодчиненныйДокумент` (2.10), which has the
/// same structure.
///
/// ```
/// use commerceml::{BusinessOperation, Document, DocumentItem, Role};
///
/// let mut order = Document::order("order-1", "1", "2026-11-02");
/// order.currency = Some("RUB".into());
/// order.items.push(DocumentItem::new("sku-1", "Синтетический товар", "2", "150.00")?);
/// order.amount = order.items_total();
/// assert_eq!(order.operation, Some(BusinessOperation::OrderGoods));
/// assert_eq!(order.role, Some(Role::Seller));
/// assert_eq!(order.amount.unwrap().to_string(), "300.00");
/// # Ok::<(), commerceml::ParseDecimalError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Document {
    /// `Ид`.
    pub id: String,
    /// `Номер`.
    pub number: String,
    /// `Дата`.
    pub date: String,
    /// `ХозОперация`.
    pub operation: Option<BusinessOperation>,
    /// `Роль` — the role of the exporting side.
    pub role: Option<Role>,
    /// `Валюта`.
    pub currency: Option<String>,
    /// `Курс` — exchange rate, a string by the standard.
    pub rate: Option<String>,
    /// `Сумма`.
    pub amount: Option<Decimal>,
    /// `Контрагенты/Контрагент`, each with a [`role`](Counterparty::role).
    pub counterparties: Vec<Counterparty>,
    /// `Время`.
    pub time: Option<String>,
    /// `СрокПлатежа`.
    pub payment_due: Option<String>,
    /// `Комментарий`.
    pub comment: Option<String>,
    /// `Налоги/Налог`.
    pub taxes: Vec<DocumentTax>,
    /// `Скидки/Скидка`.
    pub discounts: Vec<Adjustment>,
    /// `ДопРасходы/ДопРасход`.
    pub extra_charges: Vec<Adjustment>,
    /// `Склады/Склад` (since 2.07).
    pub warehouses: Vec<Warehouse>,
    /// `Товары/Товар` — document lines.
    pub items: Vec<DocumentItem>,
    /// `ЗначенияРеквизитов/ЗначениеРеквизита`, e.g. `Статус заказа`,
    /// `Метод оплаты`.
    pub requisites: Vec<Requisite>,
    /// `Подписанты/Подписант`.
    pub signatories: Vec<Signatory>,
    /// `Оплаты/Оплата` (2.09 only).
    pub payments: Vec<SitePayment>,
    /// `ПодчиненныеДокументы/ПодчиненныйДокумент` (2.10).
    pub subordinate_documents: Vec<Document>,
    /// `Номер1С` (de facto).
    pub number_1c: Option<String>,
    /// `Дата1С` (de facto).
    pub date_1c: Option<String>,
    /// `Основание` — id of the base document (de facto).
    pub basis: Option<String>,
    /// `ПометкаУдаления` (de facto).
    pub deletion_mark: Option<bool>,
    /// `НомерВерсии` (de facto).
    pub version: Option<String>,
    /// Elements not modelled by the library.
    pub extra: Vec<RawElement>,
}

impl Document {
    /// A new order (`Заказ товара`) with the seller role.
    pub fn order(id: impl Into<String>, number: impl Into<String>, date: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            number: number.into(),
            date: date.into(),
            operation: Some(BusinessOperation::OrderGoods),
            role: Some(Role::Seller),
            rate: Some("1".into()),
            ..Self::default()
        }
    }

    /// Value of the requisite with this name.
    pub fn requisite(&self, name: &str) -> Option<&str> {
        crate::RequisitesExt::requisite(self.requisites.as_slice(), name)
    }

    /// Sum of [`DocumentItem::amount`] over all lines, `None` if a line
    /// has no amount or the sum overflows.
    pub fn items_total(&self) -> Option<Decimal> {
        self.items
            .iter()
            .try_fold(Decimal::ZERO, |acc, i| acc.checked_add(i.amount?))
    }

    /// The first counterparty with the given role.
    pub fn counterparty(&self, role: &Role) -> Option<&Counterparty> {
        self.counterparties.iter().find(|c| c.role.as_ref() == Some(role))
    }
}

/// A document line: `Товар` inside `Документ`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DocumentItem {
    /// The product fields (`Ид`, `Наименование`, `БазоваяЕдиница`…).
    pub product: Product,
    /// `ИдКаталога`.
    pub catalog_id: Option<String>,
    /// `ИдКлассификатора`.
    pub classifier_id: Option<String>,
    /// `ЦенаЗаЕдиницу`.
    pub price_per_unit: Option<Decimal>,
    /// `Количество`.
    pub quantity: Option<Decimal>,
    /// `Сумма`.
    pub amount: Option<Decimal>,
    /// `Единица`, `Коэффициент`, `ДополнительныеДанные`.
    pub unit: UnitConversion,
    /// `СтранаПроисхождения`.
    pub country_of_origin: Option<String>,
    /// `ГТД` — customs declaration number.
    pub customs_declaration: Option<String>,
    /// `Налоги/Налог`.
    pub taxes: Vec<DocumentTax>,
    /// `Скидки/Скидка`.
    pub discounts: Vec<Adjustment>,
    /// `ДопРасходы/ДопРасход`.
    pub extra_charges: Vec<Adjustment>,
    /// `ДополнительныеЗначенияРеквизитов/ЗначениеРеквизита`.
    pub additional_requisites: Vec<Requisite>,
    /// `Склады/Склад` with `Количество` (since 2.07).
    pub warehouses: Vec<WarehouseQuantity>,
}

impl DocumentItem {
    /// A line with id, name, quantity and price; the amount is computed.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        quantity: &str,
        price: &str,
    ) -> Result<Self, crate::ParseDecimalError> {
        let quantity: Decimal = quantity.parse()?;
        let price: Decimal = price.parse()?;
        Ok(Self {
            product: Product {
                id: id.into(),
                name: name.into(),
                ..Product::default()
            },
            price_per_unit: Some(price),
            quantity: Some(quantity),
            amount: price.checked_mul(quantity),
            ..Self::default()
        })
    }
}

/// A tax with an amount: `Налог` inside documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DocumentTax {
    /// `Наименование`, `УчтеноВСумме`, `Акциз`.
    pub tax: Tax,
    /// `Сумма`.
    pub amount: Option<Decimal>,
    /// `Ставка`.
    pub rate: Option<String>,
}

/// A discount (`Скидка`) or an extra charge (`ДопРасход`); both have the
/// same structure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Adjustment {
    /// `Наименование`.
    pub name: String,
    /// `Сумма`.
    pub amount: Option<Decimal>,
    /// `Процент`.
    pub percent: Option<String>,
    /// `УчтеноВСумме` — whether the document amount already includes it.
    pub included_in_amount: Option<bool>,
    /// `Комментарий`.
    pub comment: Option<String>,
}

/// Quantity of a document line in one warehouse: `Склад` + `Количество`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WarehouseQuantity {
    /// The warehouse.
    pub warehouse: Warehouse,
    /// `Количество`.
    pub quantity: Option<Decimal>,
}

/// A payment made on the website: `Оплата` inside `Оплаты` (2.09 only;
/// removed in 2.10).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SitePayment {
    /// `НомерДокумента`.
    pub document_number: String,
    /// `НомерТранзакции`.
    pub transaction_number: String,
    /// `ДатаОплаты`.
    pub date: String,
    /// `СуммаОплаты`.
    pub amount: Option<Decimal>,
    /// `СпособОплаты`.
    pub method: String,
    /// `ИдСпособаОплаты`.
    pub method_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_helpers() {
        let mut o = Document::order("1", "1", "2026-01-01");
        assert_eq!(o.items_total(), Some(Decimal::ZERO));
        o.items.push(DocumentItem::new("a", "A", "3", "1.10").unwrap());
        o.items.push(DocumentItem::default());
        assert_eq!(o.items_total(), None);
        o.items.pop();
        assert_eq!(o.items_total().unwrap().to_string(), "3.30");
        o.requisites.push(Requisite::new("Статус заказа", "Новый"));
        assert_eq!(o.requisite("Статус заказа"), Some("Новый"));
        o.counterparties.push(Counterparty {
            role: Some(Role::Buyer),
            ..Counterparty::default()
        });
        assert!(o.counterparty(&Role::Buyer).is_some());
        assert!(o.counterparty(&Role::Seller).is_none());
        assert!(DocumentItem::new("a", "A", "x", "1").is_err());
    }
}
