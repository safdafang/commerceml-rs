# commerceml-rs

[![CI](https://github.com/safdafang/commerceml-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/safdafang/commerceml-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/commerceml.svg)](https://crates.io/crates/commerceml)
[![docs.rs](https://img.shields.io/docsrs/commerceml)](https://docs.rs/commerceml)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg)](#licence)

Read, write and exchange **CommerceML 2** in Rust — the XML format
1C:Enterprise uses to send catalogs, prices, stock and orders to websites —
including the HTTP exchange protocol (`1c_exchange.php?type=catalog&mode=checkauth`
and the rest).

> [!NOTE]
> CommerceML is a standard of 1C. This project is not affiliated with 1C or
> 1C-Bitrix. Only synthetic data is used in tests and examples.

| Crate | What it does |
|---|---|
| [`commerceml`](crates/commerceml) | data model, parser (whole file or streaming) and writer |
| [`commerceml-exchange`](crates/commerceml-exchange) | server side of the 1C ↔ website protocol, with an axum router |
| [`commerceml-cli`](crates/commerceml-cli) | `commerceml validate`, `stats`, `convert`, `diff`, `generate` |

## Features

* **Streaming** — one product, offer or order at a time. Streaming a 112 MB
  `import.xml` peaks at about 80 kB of heap, the same as a 4 MB file.
* **Real exports, not just the XSD** — elements that 1C configurations and CMS
  modules add (`Остатки`, `ПометкаУдаления`, `ЕдиницыИзмерения`, root-level
  `Контрагенты`, `Контейнер` …) are modelled. Anything else is kept as raw XML and
  reported as a warning with its path, line and column; unknown elements never
  stop parsing.
* **Versions 2.03–2.10**, with or without a namespace, UTF-8 or windows-1251.
* **Writer** — whole documents or section by section; `parse → write → parse`
  returns the same data. Amounts are an exact `Decimal` that keeps the written
  form (`1499.00` stays `1499.00`).
* **Validation** — required fields, duplicate ids, references to groups,
  properties, price types and warehouses that do not exist.
* **Exchange server** — sessions, `init` parameters, uploads in parts, zip
  archives, `progress` replies, size limits, protection against path traversal,
  zip-slip and zip bombs. You write authentication and what to do with the data.
* **CLI** — `commerceml validate | stats | convert | diff | generate`.

## Installation

```toml
[dependencies]
commerceml = "0.1"
```

Rust 1.88 or newer. The command-line tool: `cargo install commerceml-cli`.

## Parse an export

```rust,no_run
fn main() -> Result<(), commerceml::Error> {
    let parsed = commerceml::from_path("import.xml")?;
    for warning in &parsed.warnings {
        eprintln!("{warning}");
    }

    let info = &parsed.info;
    if let Some(classifier) = &info.classifier {
        for group in classifier.all_groups() {
            println!("group {} — {}", group.id, group.name);
        }
    }
    for product in info.products() {
        println!(
            "{} | {} | article {:?} | {} picture(s)",
            product.id,
            product.name,
            product.article,
            product.pictures.len()
        );
        if let Some(kind) = product.requisite("ВидНоменклатуры") {
            println!("  kind: {kind}");
        }
    }
    Ok(())
}
```

Prices and stock are in `offers.xml`. The `Ид` of a product variant is
`<product id>#<characteristic id>`:

```rust,no_run
fn main() -> Result<(), commerceml::Error> {
    let offers = commerceml::from_path("offers.xml")?.info;
    for offer in offers.offers() {
        let retail = offer.prices.first().and_then(|p| p.price_per_unit);
        println!("{} {:?} in stock: {:?}", offer.product_id(), retail, offer.total_quantity());
    }
    Ok(())
}
```

## Large files

```rust,no_run
use commerceml::stream::{Event, StreamReader};
use commerceml::ParseOptions;

fn main() -> Result<(), commerceml::Error> {
    let mut reader = StreamReader::open("offers.xml", ParseOptions::default())?;
    for event in reader.by_ref() {
        match event? {
            Event::OfferPackageStart(package) => println!("{} price types", package.price_types.len()),
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

Section headers (`ClassifierStart`/`End`, `CatalogStart`/`End`,
`OfferPackageStart`/`End`) bracket the items. Loading a whole file uses the same
reader, so both modes see the same data.

## Write orders for 1C

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
    println!("{} / {} bytes", utf8.len(), cp1251.len());
    Ok(())
}
```

`commerceml::write::Writer` writes section by section (`begin_catalog`,
`product`, `end_catalog` …) for exports that should not be built in memory.

## Exchange endpoint

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
            println!("{} products, {} offers", parsed.info.products().len(), parsed.info.offers().len());
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

In 1C ("Обмен с сайтом") set the site address to
`http://<host>:8080/1c_exchange.php`. A complete demo is
[`examples/server.rs`](crates/commerceml-exchange/examples/server.rs).

## Command line

```console
$ commerceml validate offers.xml
offers.xml: FAILED — 1 error(s), 1 warning(s), 1 parser warning(s) [ВерсияСхемы 2.07, UTF-8]
  warning[invalid-value] Предложение "p-lamp#c-red": negative stock -1
  error[missing-field] Предложение "p-lamp#": price of type "price-retail" has no ЦенаЗаЕдиницу
  parser warning: invalid value "не число" in <ЦенаЗаЕдиницу> at …/Цена/ЦенаЗаЕдиницу (line 97, column 7)
$ commerceml validate import.xml --format json     # exit code 1 on errors
$ commerceml stats import.xml
$ commerceml convert offers.xml --to csv -o prices.csv
$ commerceml convert import.xml --to jsonl --entity products
$ commerceml diff yesterday/import.xml today/import.xml --values
$ commerceml generate --products 100000 --out synthetic/
```

## Supported

| | |
|---|---|
| Schema versions | 2.03–2.10; structure checked against the XSDs of 2.05–2.10 |
| Root sections | `Классификатор`, `Каталог`, `ПакетПредложений`, `ИзмененияПакетаПредложений`, `Документ`; de facto `Контрагенты`, `Контейнер` |
| Files | `import.xml`, `offers.xml`, `prices.xml`, `rests.xml`, orders, `contragents.xml`, numbered parts (`import0_1.xml`) |
| Encodings | UTF-8 (with or without BOM), windows-1251, any label known to `encoding_rs` |
| Namespaces | none, `urn:1C.ru:commerceml_2`, `urn:1C.ru:commerceml_210`, prefixed |
| Protocol | `checkauth`, `init`, `file` in parts, `import` with `progress`, `deactivate`, `complete`, `query`, `success` |

## Performance

Synthetic export of 40 000 products, Intel Core i7-10750H, NVMe SSD,
Windows 11, Rust 1.97, system allocator (`cargo bench -p commerceml --features synth`):

| Task | File | Time | Throughput |
|---|---|---|---|
| Stream `import.xml` (UTF-8) | 112 MB | 1.04 s | 103 MiB/s |
| Load `import.xml` into memory | 112 MB | 1.10 s | 97 MiB/s |
| Stream `import.xml` (windows-1251) | 74 MB | 1.28 s | 55 MiB/s |
| Stream `offers.xml` | 103 MB | 1.43 s | 69 MiB/s |
| Write `import.xml` | 112 MB | 0.20 s | 539 MiB/s |

Loading a whole file takes about 1.2× its size in memory; streaming stays at
about 80 kB (`tests/memory.rs` checks it).

## Limitations

* CommerceML 1.x is not supported. Of CommerceML 3.x only `Контейнер` is
  understood; `ПользовательскиеСправочники` is kept as raw XML.
* Dates and times are kept as strings in their written form.
* Elements outside the model are kept as raw XML on products, offers, documents,
  groups, properties and counterparties; on smaller parts (addresses, prices …)
  they are only reported.
* The exchange server keeps sessions in memory: one process per endpoint.
* Behaviour of real 1C configurations was checked against published samples and
  documentation, not against a running 1C.

## Known issues in the standard

Details and sources in [docs/spec-notes.md](docs/spec-notes.md).

1. The official sample on the 1C protocol page says `ВерсияСхемы="2.08"` inside
   the `urn:1C.ru:commerceml_210` namespace, and its orders use a date-only
   `ДатаФормирования`, which is not an `xsd:dateTime`.
2. The same sample wraps representatives as `Представитель/Контрагент/…`, which
   the XSD does not allow. This library writes the 1C form and reads both.
3. `ОплатаСайт` exists only in the 2.09 XSD, although the 2.10 change list says
   it was removed in 2.10.
4. The XSD spells `Руководитель/АресРегистрации` (sic).
5. The protocol description gives two lines for the `init` answer, Bitrix
   documents four (`sessid`, `version`).
6. Real exports use elements that no XSD defines (see *Features*).

## Русский

**commerceml-rs** — библиотека на Rust для чтения, записи и обмена данными в
формате CommerceML 2 (1С ↔ сайт), включая HTTP-протокол обмена. Описание на
русском — в [README.ru.md](README.ru.md).

## Documentation

* API: [docs.rs/commerceml](https://docs.rs/commerceml),
  [docs.rs/commerceml-exchange](https://docs.rs/commerceml-exchange)
* [docs/spec-notes.md](docs/spec-notes.md) — versions, de facto extensions, protocol, open questions
* [docs/prior-art.md](docs/prior-art.md) — survey of existing implementations
* [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md)

## Licence

MIT OR Apache-2.0, at your option (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)).
