# CommerceML 2: notes on the standard

These notes record what the standard says, where real exports differ from it,
and which points stay unclear. They are the basis for the parser's behaviour.
Items marked `TODO(spec)` are open: the sources below do not settle them.

## Sources

| # | Source | Status |
|---|---|---|
| S1 | XSD of edition 2.10, published inline on [v8.1c.ru › CommerceML 2](https://v8.1c.ru/tekhnologii/obmen-dannymi-i-integratsiya/standarty-i-formaty/standarty-commerceml/commerceml-2/) | official |
| S2 | "Логическое описание ЭОИ, редакция 2, версия 10" (`logicheskoe-opisanie-commerceml-2-10.doc`, same page, 2018) | official |
| S3 | "Лист изменений для перехода с версии 9 на версию 10" (`commerceml_2_10.pdf`, same page) | official |
| S4 | [Протокол обмена с сайтом](https://v8.1c.ru/tekhnologii/obmen-dannymi-i-integratsiya/standarty-i-formaty/protokol-obmena-s-saytom/), with sample `import.xml` (2.08) and orders (2.03) | official |
| S5 | 1C-Bitrix developer docs: [catalog exchange algorithm](https://dev.1c-bitrix.ru/api_help/sale/algorithms/data_2_site.php), [orders](https://dev.1c-bitrix.ru/api_help/sale/algorithms/doc_from_site.php), and their description of `import.xml`, `offers.xml`, `prices.xml`, `rests.xml`, `contragents.xml`, `documents.xml`, `references.xml` | vendor (Bitrix is the protocol's co-author) |
| S6 | XSD files 2.05, 2.06, 2.07, 2.09, 2.10 shipped in the Webasyst `cml1c` plugin; XSD 2.08 in `dobromyslov/commerceml-parser` | third-party copies; the 2.10 copy is structurally identical to S1 |
| S7 | Sample exports in `dobromyslov/commerceml-parser` (`ВерсияСхемы="2.07"`) | real-world sample |

None of these files are vendored in this repository.

## Document model

The root element `КоммерческаяИнформация` has two required attributes,
`ВерсияСхемы` (schema version) and `ДатаФормирования` (`xsd:dateTime`), an
optional `Классификатор`, and then a choice of `Каталог`, `ПакетПредложений`,
any number of `Документ`, or any number of `ИзмененияПакетаПредложений` (S1).

In practice one root carries several of these: `offers.xml` repeats a
`Классификатор` header before the `ПакетПредложений` (S7). The parser accepts
any combination and order.

| Root child | Typical file | Purpose |
|---|---|---|
| `Классификатор` | `import.xml` | groups, properties, price types (+ de facto warehouses, units, categories) |
| `Каталог` | `import.xml` | products (`Товар`) |
| `ПакетПредложений` | `offers.xml`, `prices.xml`, `rests.xml` | offers (`Предложение`) with prices and stock |
| `ИзмененияПакетаПредложений` | — | price/stock changes only (since 2.07) |
| `Документ` | orders | business documents (orders, invoices, …) |
| `Контрагенты` | `contragents.xml` | counterparties (not in the XSD, S5) |
| `Контейнер` | `documents.xml` | wraps `Документ` elements in CommerceML 3.x exchanges (S5) |
| `ПользовательскиеСправочники` | `references.xml` | user reference books (not in the XSD, S5) |

Exports are often split into numbered files, e.g. `import0_1.xml`,
`offers0_1.xml` (S7). S5 recommends a random suffix in file names.

## Version history (2.05 → 2.10)

Derived by diffing the structure of the XSD files (S1, S6). Edition 2.04 is
**not covered** by any source found. `TODO(spec)` find the 2.04 XSD and check
what changed between 2.04 and 2.05.

| From → to | Changes |
|---|---|
| 2.05 → 2.06 | `Документ` turned from an inline element into a named complex type; identifier type renamed from the misspelt `ИдентфикаторГлобальныйТип` to `ИдентификаторГлобальныйТип`. No change in instance documents. |
| 2.06 → 2.07 | `ИзмененияПакетаПредложений` added (offer changes: `ИдХарактеристики`, `КодЕдиницыИзмерения`, `Склады`, `Цены`, `Количество`); `Склады` added to `Документ` and to document lines (`Товар/Склады/Склад/Количество`); `ПакетПредложений/Склады` added; stock per warehouse factored into `ОстаткиПоСкладам` (attributes `ИдСклада`, `КоличествоНаСкладе`); `Цена/ИдКаталога` added; `ТипЦены/Наименование` made optional; `КонтактТип` gained `Координаты на карте` and `Прочее`; `ХозОперацияТип` gained `Отчет о списании комиссионного товара`. |
| 2.07 → 2.08 | `БазоваяЕдиница/@Код` type changed from `Строка3` to `ЕдиницаТип` (longer unit codes allowed). |
| 2.08 → 2.09 | `Документ/Оплаты` (`ОплатаСайт`: payment number, transaction, date, amount, method) added. |
| 2.09 → 2.10 | `Оплаты` removed again; `ПодчиненныеДокументы/ПодчиненныйДокумент` added: documents created from the main one, e.g. payments and shipments for an order (S3). |

The namespace in every XSD is `urn:1C.ru:commerceml_2`, but the official
sample (S4) uses `urn:1C.ru:commerceml_210`, and many exports have no
namespace. **The parser matches elements by local name and ignores namespaces.**

`ВерсияСхемы` in files does not always match the content: S4's sample says
`2.08` inside a `commerceml_210` namespace; Bitrix documents describe 3.1 and
fall back to "2.1" when no version is negotiated. The parser records the
version string but never relies on it for parsing.

## De facto extensions (not in any XSD)

Seen in official samples (S4), Bitrix docs (S5) or real files (S7). The
library models all of them as optional fields.

| Element | Where | Meaning |
|---|---|---|
| `Классификатор/Склады/Склад` | import.xml (S5) | warehouses |
| `Классификатор/ЕдиницыИзмерения/ЕдиницаИзмерения` (`Код`, `НаименованиеПолное`, `НаименованиеКраткое`, `МеждународноеСокращение`) | import.xml (S4) | unit catalogue |
| `Классификатор/Категории/Категория` (`Ид`, `Наименование`, `Свойства`) | import.xml (S4) | product categories; `Товар/Категория` refers to one |
| `ПометкаУдаления` | groups, properties, price types, warehouses, products, offers, counterparties, documents (S5) | deletion mark |
| `НомерВерсии` | same (S5) | object version |
| `Свойство/Внешний`, `Справочник/Картинка` | S5 | property stored in a reference book; picture of a value |
| `Товар/Вес` | S5 | weight |
| `Товар/Изготовитель` | S7 | manufacturer, also in the XSD via group `Производитель` |
| `Предложение/Остатки/Остаток` with `Количество` or `Склад/{Ид,Количество}` | rests.xml (S5) | stock, newer replacement of `Склад/@КоличествоНаСкладе` |
| `Документ/Номер1С`, `Дата1С`, `Основание` | documents.xml (S5) | numbering in 1C, base document |
| `Представитель/Контрагент/{Отношение,Ид,Наименование}` | orders (S4, S5) | representative wrapped in an extra `Контрагент` element |
| `Контрагент/КодПоОКПО` | contragents.xml (S5) | alias of `ОКПО` |

Elements outside this list and outside the XSD are reported as warnings
and kept as raw XML on the owning entity, so they survive a round trip.

## Writing decisions

- Elements are written in XSD order. De facto elements go after the
  standard ones of the same parent (`Категория` right after `Группы`).
- `Руководитель` keeps the XSD spelling `АресРегистрации` (sic) on output;
  both spellings are read.
- When a counterparty has legal-entity details (`ОфициальноеНаименование`),
  `ИНН`/`КПП` are written in the legal-entity block, otherwise in the
  individual block.
- Text is written verbatim; `` becomes `&#13;` so it survives XML
  end-of-line normalization. Characters not allowed in XML 1.0 are removed
  on input (with a warning) and never written.
- In windows-1251 output, characters the encoding lacks are written as
  numeric character references.

## Identifiers

- `Ид` of an offer may be `<product id>#<characteristic id>` (S5, S7). An
  offer whose `Ид` has no `#` refers to the product itself. A trailing `#`
  also occurs (`b21db898-…#` in S7, and `ПакетПредложений/Ид` ending with `#`
  in S5). The library exposes the raw `Ид` plus helpers that split it.
- `Ид` values are GUIDs in 1C 8 exports, but S4 has order ids like
  `1#admin#Петров Петр` and `ORDER_DELIVERY` (delivery as a line item). The
  library treats every id as an opaque string.

## Data types

| XSD type | Library type | Notes |
|---|---|---|
| `СуммаТип`, `КоличествоТип` (`xsd:decimal`) | `Decimal` (own, exact) | lexical form is kept (`1.00` stays `1.00`). A comma as decimal separator is accepted on input. |
| `КоэффициентТип` (`xsd:string`!) | `String` | used for `Ставка`, `Курс`, `Коэффициент`; real values include `Без налога`. |
| `ДатаТип`, `ВремяТип`, `ДатаВремяТип` | `String` | lexical form kept; values outside the XSD form occur in `ЗначениеРеквизита` (`2007-10-16 15:44:47`). |
| `ПереключательТип` (`xsd:boolean`) | `bool` | `true`/`false`/`1`/`0`; anything else becomes a warning. |
| enumerations (`РольТип`, `ХозОперацияТип`, `СтатусТип`, `ТипЗначенийТип`, `КонтактТип`, `АдресТип`, `ПолТип`, `ОбязательностьСвойствТип`) | Rust enums with an `Other(String)` variant | real values deviate (`Электронная почта`, `ЗаказТовара` in S3's list vs `Заказ товара` in S1). |

## Encodings

UTF-8 is the norm for 1C 8 exports. Windows-1251 appears in 1C 7.7-era exports
and in Bitrix order exports for CommerceML 2 (`TODO(spec)` confirm which modern
1C configurations still produce windows-1251). The parser picks the encoding
from the BOM or the XML declaration and transcodes to UTF-8 on the fly.

## Exchange protocol (S4, S5)

All requests go to one URL with query parameters `type` (`catalog` or `sale`)
and `mode`. 1C always initiates.

| Step | Request | Response |
|---|---|---|
| A | `mode=checkauth`, HTTP Basic auth | `success\n<cookie name>\n<cookie value>` (S4); Bitrix adds `sessid=…` and a server timestamp on lines 4–5 (S5) |
| B | `mode=init` (Bitrix: `&version=<cml version>`) | `zip=yes\|no\nfile_limit=<bytes>` (S4); Bitrix adds `sessid=…` and `version=…` (S5) |
| C | `mode=file&filename=…`, POST body = file or part of it | `success`, or `failure\n<reason>` |
| D | `mode=import&filename=…` (catalog), repeated | `progress\n<status>` (call again), `success`, or `failure\n<reason>` |
| — | `mode=deactivate&timestamp=…` (Bitrix, full exchange only) | deactivate objects older than the timestamp |
| — | `mode=complete` (Bitrix) | end of the exchange |
| sale | `mode=query` | CommerceML document with orders; then `mode=success` |
| sale | `mode=file&filename=…` | orders changed in 1C; `success` plus optional notes |

Every request after `checkauth` carries the cookie it returned. Files larger
than `file_limit` are sent in parts, appended in order. With `zip=yes` the
files arrive as one zip archive; `mode=import` then names files inside it.

Open points:

- `TODO(spec)` S4 says the init response has *two* lines, S5 four. Whether old
  1C versions choke on the extra lines is not documented; the server sends
  the Bitrix form (extra lines are ignored by line-based readers).
- `TODO(spec)` how 1C reacts when the server answers `version=` with a lower
  version than requested is not documented. The server answers with the
  requested version if it is 2.x, else `2.10`.
- `TODO(spec)` the encoding of plain-text protocol responses is not specified.
  The server answers UTF-8 with an explicit `charset`.
- `TODO(spec)` S4 does not say whether `mode=import` is used for `type=sale`.
  The server stores sale files on `mode=file`, calls the handler right away,
  and answers `success` to a later `mode=import`.
- `TODO(spec)` S4 shows `Представитель/Контрагент/…` in orders, which is not
  valid against S1. The writer follows S4 for orders because that is what 1C
  itself publishes as the expected input; the parser accepts both forms.
- `TODO(spec)` which `ВерсияСхемы` and encoding 1C expects in the `mode=query`
  answer for each configuration. The writer defaults to `2.10`, UTF-8; both
  are configurable.
- `TODO(spec)` whether 1C handles HTTP 401 on failed `checkauth` better than
  `200` with `failure`. The server answers 401 with `failure` in the body
  and a `WWW-Authenticate` header.
- `TODO(spec)` the encoding of file names inside zip archives from 1C. Only
  ASCII names (`import.xml`, `import_files/…`) were seen; non-UTF-8 names
  are decoded by the `zip` crate as CP437.
- `TODO(spec)` whether 1C ever splits a `type=sale` file into parts. If it does,
  disable `sale_import_on_file` and process on `mode=import`.
- The session is identified by the cookie from `checkauth` or, as in Bitrix,
  by `sessid` in the query. Percent-encoded file names that are not valid
  UTF-8 are read as windows-1251.
