//! A demo website endpoint for 1C: keeps the catalog and prices in memory
//! and offers one pending order.
//!
//! ```sh
//! COMMERCEML_LOGIN=1c COMMERCEML_PASSWORD=secret \
//!     cargo run -p commerceml-exchange --example server
//! ```
//!
//! In 1C (e.g. "Обмен с сайтом" in УТ/УНФ/КА) set the site address to
//! `http://<this host>:8080/1c_exchange.php` with the same login and
//! password. To try it without 1C:
//!
//! ```sh
//! curl -u 1c:secret "http://127.0.0.1:8080/1c_exchange.php?type=catalog&mode=checkauth"
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use commerceml::stream::Event;
use commerceml::{CommercialInfo, Decimal, Document, DocumentItem, ParseOptions};
use commerceml_exchange::{
    Credentials, Exchange, ExchangeConfig, ExchangeHandler, FsStorage, HandlerError, ImportStatus,
    ImportedFile, SessionInfo,
};

#[derive(Debug, Default, Clone)]
struct Item {
    name: String,
    price: Option<Decimal>,
    stock: Option<Decimal>,
}

/// The site's data; shared with blocking tasks through an `Arc`.
#[derive(Default)]
struct Catalog {
    items: Mutex<HashMap<String, Item>>,
    orders_sent: Mutex<bool>,
}

struct DemoShop {
    credentials: Credentials,
    catalog: Arc<Catalog>,
}

impl Catalog {
    /// Reads a file in constant memory and updates the in-memory catalog.
    fn apply(&self, file: &ImportedFile) -> Result<(usize, Vec<String>), HandlerError> {
        let mut reader = file.stream(&ParseOptions::default())?;
        let mut changed = 0;
        for event in reader.by_ref() {
            let mut items = self.items.lock().unwrap();
            match event? {
                Event::Product(p) => {
                    p.name
                        .trim()
                        .clone_into(&mut items.entry(p.id.clone()).or_default().name);
                    changed += 1;
                }
                Event::Offer(o) => {
                    let item = items.entry(o.product_id().to_owned()).or_default();
                    if item.name.is_empty() {
                        o.product.name.trim().clone_into(&mut item.name);
                    }
                    item.price = o.prices.first().and_then(|p| p.price_per_unit).or(item.price);
                    item.stock = o.total_quantity().or(item.stock);
                    changed += 1;
                }
                _ => {}
            }
        }
        let warnings = reader.take_warnings().iter().map(ToString::to_string).collect();
        Ok((changed, warnings))
    }
}

impl ExchangeHandler for DemoShop {
    async fn authenticate(&self, login: &str, password: &str) -> bool {
        let ok = self.credentials.check(login, password);
        println!("checkauth {login}: {}", if ok { "ok" } else { "rejected" });
        ok
    }

    async fn import_catalog(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
        if !file.is_xml() {
            println!("stored {}", file.name);
            return Ok(ImportStatus::Done);
        }
        let name = file.name.clone();
        let catalog = Arc::clone(&self.catalog);
        let (changed, warnings) = tokio::task::spawn_blocking(move || catalog.apply(&file)).await??;
        println!("{name}: {changed} products/offers, {} warning(s)", warnings.len());
        for w in warnings.iter().take(5) {
            println!("  {w}");
        }
        Ok(ImportStatus::Done)
    }

    async fn query_orders(&self, _session: SessionInfo) -> Result<CommercialInfo, HandlerError> {
        let mut info = CommercialInfo::default();
        if *self.catalog.orders_sent.lock().unwrap() {
            return Ok(info);
        }
        let items = self.catalog.items.lock().unwrap().clone();
        let mut order = Document::order("demo-order-1", "1", "2026-11-02");
        order.currency = Some("RUB".into());
        if let Some((id, item)) = items.iter().find(|(_, i)| i.price.is_some()) {
            let mut line = DocumentItem::new(id.clone(), item.name.clone(), "1", "0")?;
            line.price_per_unit = item.price;
            line.amount = item.price;
            order.items.push(line);
        }
        order.amount = order.items_total();
        info.documents.push(order);
        Ok(info)
    }

    async fn orders_received(&self, _session: SessionInfo) -> Result<(), HandlerError> {
        *self.catalog.orders_sent.lock().unwrap() = true;
        println!("1C saved the orders");
        Ok(())
    }

    async fn import_orders(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
        let name = file.name.clone();
        let parsed = tokio::task::spawn_blocking(move || file.parse()).await??;
        for d in &parsed.info.documents {
            println!(
                "{name}: order {} status {:?}",
                d.number,
                d.requisite("Статус заказа")
            );
        }
        Ok(ImportStatus::Done)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let login = std::env::var("COMMERCEML_LOGIN").unwrap_or_else(|_| "1c".into());
    let password = std::env::var("COMMERCEML_PASSWORD").unwrap_or_else(|_| "secret".into());
    let addr = std::env::var("COMMERCEML_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let shop = DemoShop {
        credentials: Credentials::new(login, password),
        catalog: Arc::default(),
    };
    let storage = FsStorage::new(std::env::temp_dir().join("commerceml-exchange-demo"))?;
    let exchange = Exchange::new(shop, storage, ExchangeConfig::default());
    let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("listening on http://{addr}/1c_exchange.php");
    axum::serve(listener, app).await?;
    Ok(())
}
