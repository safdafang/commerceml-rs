# commerceml-rs

[![CI](https://github.com/safdafang/commerceml-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/safdafang/commerceml-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/commerceml.svg)](https://crates.io/crates/commerceml)
[![docs.rs](https://img.shields.io/docsrs/commerceml)](https://docs.rs/commerceml)
[![Лицензия: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg)](#лицензия)

Чтение, запись и обмен данными в формате **CommerceML 2** на Rust — XML, в
котором 1С:Предприятие передаёт сайтам каталоги, цены, остатки и заказы, —
вместе с HTTP-протоколом обмена (`1c_exchange.php?type=catalog&mode=checkauth`
и остальные режимы).

> [!NOTE]
> CommerceML — стандарт фирмы «1С». Проект не связан с «1С» и «1С-Битрикс».
> В тестах и примерах используются только синтетические данные.

| Крейт | Назначение |
|---|---|
| [`commerceml`](crates/commerceml) | модель данных, парсер (целиком или потоково) и генератор XML |
| [`commerceml-exchange`](crates/commerceml-exchange) | серверная часть протокола обмена 1С ↔ сайт, роутер для axum |
| [`commerceml-cli`](crates/commerceml-cli) | `commerceml validate`, `stats`, `convert`, `diff`, `generate` |

## Возможности

* **Потоковый разбор** — по одному товару, предложению или заказу. Пик кучи
  при разборе `import.xml` на 112 МБ — около 80 КБ, как и для файла на 4 МБ.
* **Реальные выгрузки, а не только XSD** — элементы, которые добавляют
  конфигурации 1С и модули CMS (`Остатки`, `ПометкаУдаления`, `ЕдиницыИзмерения`,
  `Контрагенты` в корне, `Контейнер` …), поддержаны. Остальное сохраняется как
  исходный XML и попадает в предупреждения с путём, строкой и столбцом;
  неизвестные элементы не прерывают разбор.
* **Версии 2.03–2.10**, с пространством имён и без, UTF-8 и windows-1251.
* **Запись** — целиком или по разделам; «разобрать → записать → разобрать»
  возвращает те же данные. Суммы — точный `Decimal`, сохраняющий запись
  (`1499.00` остаётся `1499.00`).
* **Проверка** — обязательные поля, повторяющиеся Ид, ссылки на несуществующие
  группы, свойства, типы цен и склады.
* **Сервер обмена** — сессии, параметры `init`, загрузка частями, zip-архивы,
  ответы `progress`, лимиты размеров, защита от выхода за каталог, zip-slip и
  zip-бомб. Вы пишете только проверку логина и обработку данных.
* **Утилита** — `commerceml validate | stats | convert | diff | generate`.

## Установка

```toml
[dependencies]
commerceml = "0.1"
```

Нужен Rust 1.88 или новее. Утилита: `cargo install commerceml-cli`.

## Разбор выгрузки

```rust,no_run
fn main() -> Result<(), commerceml::Error> {
    let parsed = commerceml::from_path("import.xml")?;
    for warning in &parsed.warnings {
        eprintln!("{warning}");
    }

    let info = &parsed.info;
    if let Some(classifier) = &info.classifier {
        for group in classifier.all_groups() {
            println!("группа {} — {}", group.id, group.name);
        }
    }
    for product in info.products() {
        println!(
            "{} | {} | артикул {:?} | картинок: {}",
            product.id,
            product.name,
            product.article,
            product.pictures.len()
        );
        if let Some(kind) = product.requisite("ВидНоменклатуры") {
            println!("  вид: {kind}");
        }
    }
    Ok(())
}
```

Цены и остатки — в `offers.xml`. У характеристик `Ид` имеет вид
`<ид товара>#<ид характеристики>`:

```rust,no_run
fn main() -> Result<(), commerceml::Error> {
    let offers = commerceml::from_path("offers.xml")?.info;
    for offer in offers.offers() {
        let retail = offer.prices.first().and_then(|p| p.price_per_unit);
        println!("{} {:?} в наличии: {:?}", offer.product_id(), retail, offer.total_quantity());
    }
    Ok(())
}
```

| XML | Rust |
|---|---|
| `КоммерческаяИнформация` | `CommercialInfo` |
| `Классификатор`, `Группа`, `Свойство`, `ТипЦены`, `Склад` | `Classifier`, `Group`, `Property`, `PriceType`, `Warehouse` |
| `Каталог`, `Товар` | `Catalog`, `Product` |
| `ПакетПредложений`, `Предложение`, `Цена` | `OfferPackage`, `Offer`, `Price` |
| `Остаток`, `Склад/@КоличествоНаСкладе` | `Rest`, `WarehouseStock` |
| `Документ`, `Товар` в документе | `Document`, `DocumentItem` |
| `Контрагент`, `ЗначениеРеквизита` | `Counterparty`, `Requisite` |

## Большие файлы

```rust,no_run
use commerceml::stream::{Event, StreamReader};
use commerceml::ParseOptions;

fn main() -> Result<(), commerceml::Error> {
    let mut reader = StreamReader::open("offers.xml", ParseOptions::default())?;
    for event in reader.by_ref() {
        match event? {
            Event::OfferPackageStart(package) => println!("типов цен: {}", package.price_types.len()),
            Event::Offer(offer) => println!("{}", offer.id()),
            _ => {}
        }
    }
    for warning in reader.take_warnings() {
        eprintln!("{warning}");
    }
    Ok(())
}
```

Заголовки разделов (`ClassifierStart`/`End`, `CatalogStart`/`End`,
`OfferPackageStart`/`End`) обрамляют элементы. Полная загрузка использует тот
же читатель, поэтому оба режима видят одинаковые данные.

## Заказы для 1С

```rust,no_run
use commerceml::{CommercialInfo, Document, DocumentItem, Encoding, WriteOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut order = Document::order("site-1001", "1001", "2026-11-02");
    order.currency = Some("RUB".into());
    order.items.push(DocumentItem::new("p-chair", "Стул складной", "2", "999.50")?);
    order.amount = order.items_total();

    let mut info = CommercialInfo::new("2026-11-02T12:00:00");
    info.documents.push(order);

    let utf8: String = commerceml::to_string(&info)?;
    let cp1251: Vec<u8> = WriteOptions::default().encoding(Encoding::WINDOWS_1251).to_vec(&info)?;
    println!("{} / {} байт", utf8.len(), cp1251.len());
    Ok(())
}
```

Для выгрузок, которые не стоит собирать в памяти, есть
`commerceml::write::Writer` (`begin_catalog`, `product`, `end_catalog` …).

## Точка обмена с 1С

```rust,no_run
use commerceml_exchange::{
    Credentials, Exchange, ExchangeConfig, ExchangeHandler, FsStorage, HandlerError, ImportStatus, ImportedFile,
};

struct Shop {
    credentials: Credentials,
}

impl ExchangeHandler for Shop {
    async fn authenticate(&self, login: &str, password: &str) -> bool {
        self.credentials.check(login, password)
    }

    async fn import_catalog(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
        if file.is_xml() {
            let parsed = tokio::task::spawn_blocking(move || file.parse()).await??;
            println!("товаров: {}, предложений: {}", parsed.info.products().len(), parsed.info.offers().len());
        }
        Ok(ImportStatus::Done)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shop = Shop { credentials: Credentials::new("1c", "secret") };
    let exchange = Exchange::new(shop, FsStorage::new("exchange-data")?, ExchangeConfig::default());
    let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange);
    axum::serve(tokio::net::TcpListener::bind("0.0.0.0:8080").await?, app).await?;
    Ok(())
}
```

В 1С («Обмен с сайтом») укажите адрес `http://<хост>:8080/1c_exchange.php`.
Полный пример — [`examples/server.rs`](crates/commerceml-exchange/examples/server.rs).

## Командная строка

```console
$ commerceml validate import.xml offers.xml        # код возврата 1 при ошибках
$ commerceml stats import.xml
$ commerceml convert offers.xml --to csv -o prices.csv
$ commerceml convert import.xml --to jsonl --entity products
$ commerceml diff yesterday/import.xml today/import.xml --values
$ commerceml generate --products 100000 --out synthetic/
```

## Что поддерживается

| | |
|---|---|
| Версии схемы | 2.03–2.10; структура сверена с XSD 2.05–2.10 |
| Разделы | `Классификатор`, `Каталог`, `ПакетПредложений`, `ИзмененияПакетаПредложений`, `Документ`; де-факто `Контрагенты`, `Контейнер` |
| Файлы | `import.xml`, `offers.xml`, `prices.xml`, `rests.xml`, заказы, `contragents.xml`, части (`import0_1.xml`) |
| Кодировки | UTF-8 (с BOM и без), windows-1251, любые метки `encoding_rs` |
| Пространства имён | без, `urn:1C.ru:commerceml_2`, `urn:1C.ru:commerceml_210`, с префиксами |
| Протокол | `checkauth`, `init`, `file` частями, `import` с `progress`, `deactivate`, `complete`, `query`, `success` |

## Производительность

Синтетическая выгрузка на 40 000 товаров, Intel Core i7-10750H, NVMe SSD,
Windows 11, Rust 1.97, системный аллокатор (`cargo bench -p commerceml --features synth`):

| Задача | Файл | Время | Скорость |
|---|---|---|---|
| Потоковый разбор `import.xml` (UTF-8) | 112 МБ | 1,04 с | 103 МиБ/с |
| Полная загрузка `import.xml` | 112 МБ | 1,10 с | 97 МиБ/с |
| Потоковый разбор `import.xml` (windows-1251) | 74 МБ | 1,28 с | 55 МиБ/с |
| Потоковый разбор `offers.xml` | 103 МБ | 1,43 с | 69 МиБ/с |
| Запись `import.xml` | 112 МБ | 0,20 с | 539 МиБ/с |

Полная загрузка занимает в памяти примерно 1,2× размера файла; потоковый
разбор — около 80 КБ (это проверяет `tests/memory.rs`).

## Ограничения

* CommerceML 1.x не поддерживается. Из CommerceML 3.x понимается только
  `Контейнер`; `ПользовательскиеСправочники` сохраняются как XML.
* Даты и время хранятся строками в исходной записи.
* Неизвестные элементы сохраняются как XML у товаров, предложений, документов,
  групп, свойств и контрагентов; во вложенных частях (адреса, цены …) только
  попадают в предупреждения.
* Сервер обмена хранит сессии в памяти: один процесс на точку обмена.
* Поведение конфигураций 1С проверено по опубликованным примерам и
  документации, а не на работающей 1С.

## Известные проблемы стандарта

Подробности и источники — в [docs/spec-notes.md](docs/spec-notes.md).

1. Официальный пример на странице протокола 1С указывает `ВерсияСхемы="2.08"`
   внутри пространства имён `urn:1C.ru:commerceml_210`, а в заказах
   `ДатаФормирования` содержит только дату, что не соответствует `xsd:dateTime`.
2. В том же примере представитель записан как `Представитель/Контрагент/…`,
   чего XSD не допускает. Библиотека пишет так же, как 1С, а читает обе формы.
3. `ОплатаСайт` есть только в XSD 2.09, хотя лист изменений 2.10 говорит, что
   его удалили в 2.10.
4. В XSD написано `Руководитель/АресРегистрации` (sic).
5. Описание протокола даёт две строки ответа на `init`, документация Битрикс —
   четыре (`sessid`, `version`).
6. Реальные выгрузки используют элементы, которых нет ни в одной XSD (см.
   *Возможности*).

## Документация

* API: [docs.rs/commerceml](https://docs.rs/commerceml),
  [docs.rs/commerceml-exchange](https://docs.rs/commerceml-exchange)
* [docs/spec-notes.md](docs/spec-notes.md) — версии, расширения де-факто, протокол, открытые вопросы
* [docs/prior-art.md](docs/prior-art.md) — обзор существующих реализаций
* [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md)

## Лицензия

MIT OR Apache-2.0 на ваш выбор (см. [LICENSE-MIT](LICENSE-MIT) и
[LICENSE-APACHE](LICENSE-APACHE)).
