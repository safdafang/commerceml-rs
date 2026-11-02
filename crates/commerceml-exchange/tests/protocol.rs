//! The whole exchange as 1C performs it, through the axum router.

use std::io::{Cursor, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use commerceml::{CommercialInfo, Document, DocumentItem, Encoding};
use commerceml_exchange::{
    Credentials, Exchange, ExchangeConfig, ExchangeHandler, ExchangeType, HandlerError, ImportStatus,
    ImportedFile, MemoryStorage, SessionInfo,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Default)]
struct Shop {
    imported: Mutex<Vec<(String, usize)>>,
    pictures: AtomicUsize,
    progress_first: AtomicBool,
    orders_received: AtomicBool,
    sale_files: Mutex<Vec<(String, usize)>>,
    /// `timestamp` of the last `deactivate`, `""` if it had none.
    deactivated: Mutex<Option<String>>,
    completed: AtomicBool,
}

impl ExchangeHandler for Shop {
    async fn authenticate(&self, login: &str, password: &str) -> bool {
        Credentials::new("1c", "secret").check(login, password)
    }

    async fn import_catalog(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
        if !file.is_xml() {
            self.pictures.fetch_add(1, Ordering::SeqCst);
            return Ok(ImportStatus::Done);
        }
        if self.progress_first.swap(false, Ordering::SeqCst) {
            return Ok(ImportStatus::Progress("загружено 50%".into()));
        }
        let name = file.name.clone();
        let parsed = tokio::task::spawn_blocking(move || file.parse()).await??;
        let count = parsed.info.products().len() + parsed.info.offers().len();
        self.imported.lock().unwrap().push((name, count));
        Ok(ImportStatus::Done)
    }

    async fn deactivate(&self, _session: SessionInfo, timestamp: Option<String>) -> Result<(), HandlerError> {
        *self.deactivated.lock().unwrap() = Some(timestamp.unwrap_or_default());
        Ok(())
    }

    async fn complete(&self, _session: SessionInfo, _kind: ExchangeType) -> Result<(), HandlerError> {
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn query_orders(&self, _session: SessionInfo) -> Result<CommercialInfo, HandlerError> {
        let mut order = Document::order("site-7", "7", "2026-11-02");
        order
            .items
            .push(DocumentItem::new("p-1", "Синтетический стул", "2", "100.00")?);
        order.amount = order.items_total();
        let mut info = CommercialInfo::default();
        info.documents.push(order);
        Ok(info)
    }

    async fn orders_received(&self, _session: SessionInfo) -> Result<(), HandlerError> {
        self.orders_received.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn import_orders(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
        let name = file.name.clone();
        let parsed = tokio::task::spawn_blocking(move || file.parse()).await??;
        self.sale_files
            .lock()
            .unwrap()
            .push((name, parsed.info.documents.len()));
        Ok(ImportStatus::Done)
    }
}

struct Client {
    app: Router,
    cookie: Option<String>,
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn lines(&self) -> Vec<String> {
        self.text().lines().map(str::to_owned).collect()
    }
}

const BASIC: &str = "Basic MWM6c2VjcmV0";

impl Client {
    fn new(config: ExchangeConfig) -> (Self, Exchange<Shop>) {
        let exchange = Exchange::new(Shop::default(), MemoryStorage::new(), config);
        let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange.clone());
        (Self { app, cookie: None }, exchange)
    }

    async fn call(&self, query: &str, auth: Option<&str>, body: Vec<u8>) -> Reply {
        let mut req = Request::post(format!("/1c_exchange.php?{query}"));
        if let Some(a) = auth {
            req = req.header(header::AUTHORIZATION, a);
        }
        if let Some(c) = &self.cookie {
            req = req.header(header::COOKIE, c);
        }
        let resp = self
            .app
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn get(&self, query: &str) -> Reply {
        self.call(query, None, Vec::new()).await
    }

    async fn login(&mut self, kind: &str) -> Vec<String> {
        let r = self
            .call(&format!("type={kind}&mode=checkauth"), Some(BASIC), Vec::new())
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
        let lines = r.lines();
        assert_eq!(lines[0], "success");
        self.cookie = Some(format!("{}={}", lines[1], lines[2]));
        lines
    }
}

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../commerceml/tests/fixtures")
        .join(name);
    std::fs::read(path).unwrap()
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[tokio::test]
async fn authentication() {
    let (mut c, _) = Client::new(ExchangeConfig::default());
    let r = c.get("type=catalog&mode=checkauth").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.headers.contains_key(header::WWW_AUTHENTICATE));
    let wrong = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("1c:nope")
    );
    let r = c
        .call("type=catalog&mode=checkauth", Some(&wrong), Vec::new())
        .await;
    assert_eq!(
        (r.status, r.lines()[0].as_str()),
        (StatusCode::UNAUTHORIZED, "failure")
    );

    let r = c.get("type=catalog&mode=init").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "no session yet");

    let r = c
        .call("type=catalog&mode=checkauth", Some(BASIC), Vec::new())
        .await;
    let set_cookie = r.headers[header::SET_COOKIE].to_str().unwrap().to_owned();
    assert!(set_cookie.starts_with("CML_EXCHANGE=") && set_cookie.contains("HttpOnly"));
    let lines = c.login("catalog").await;
    assert_eq!(lines[1], "CML_EXCHANGE");
    assert_eq!(lines[2].len(), 32);
    assert_eq!(lines[3], format!("sessid={}", lines[2]));
    assert!(lines[4].starts_with("timestamp="));

    let token = lines[2].clone();
    c.cookie = None;
    let r = c.get(&format!("type=catalog&mode=init&sessid={token}")).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = c.get("type=catalog&mode=init&sessid=forged").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn catalog_exchange_with_parts_and_progress() {
    let (mut c, exchange) = Client::new(ExchangeConfig::default().zip(false).file_limit(4096));
    c.login("catalog").await;
    let init = c.get("type=catalog&mode=init&version=2.08").await.lines();
    assert_eq!(init[0], "zip=no");
    assert_eq!(init[1], "file_limit=4096");
    assert!(init[2].starts_with("sessid="));
    assert_eq!(init[3], "version=2.08");

    let import = fixture("import_2_10.xml");
    assert!(import.len() > 4096);
    for part in import.chunks(4000) {
        let r = c
            .call("type=catalog&mode=file&filename=import.xml", None, part.to_vec())
            .await;
        assert_eq!(r.text(), "success\n");
    }
    let r = c
        .call(
            "type=catalog&mode=file&filename=import_files/ab/p-chair_1.jpg",
            None,
            b"jpeg".to_vec(),
        )
        .await;
    assert_eq!(r.text(), "success\n");

    exchange.handler().progress_first.store(true, Ordering::SeqCst);
    let r = c.get("type=catalog&mode=import&filename=import.xml").await;
    assert_eq!(r.lines(), ["progress", "загружено 50%"]);
    let r = c.get("type=catalog&mode=import&filename=import.xml").await;
    assert_eq!(r.text(), "success\n");
    let r = c
        .get("type=catalog&mode=import&filename=import_files%2Fab%2Fp-chair_1.jpg")
        .await;
    assert_eq!(r.text(), "success\n");
    assert_eq!(
        *exchange.handler().imported.lock().unwrap(),
        [("import.xml".to_owned(), 3)]
    );
    assert_eq!(exchange.handler().pictures.load(Ordering::SeqCst), 1);

    let r = c.get("type=catalog&mode=import&filename=offers.xml").await;
    assert_eq!(r.lines(), ["failure", "offers.xml was not uploaded"]);

    let r = c
        .call(
            "type=catalog&mode=file&filename=big.xml",
            None,
            vec![b'x'; 200 * 1024],
        )
        .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE);
    let r = c
        .call("type=catalog&mode=file&filename=a.zip", None, b"PK".to_vec())
        .await;
    assert_eq!(r.lines()[0], "failure");

    let r = c.get("type=catalog&mode=deactivate&timestamp=1700000000").await;
    assert_eq!(r.text(), "success\n");
    assert_eq!(
        exchange.handler().deactivated.lock().unwrap().as_deref(),
        Some("1700000000")
    );
    let r = c.get("type=catalog&mode=complete").await;
    assert_eq!(r.text(), "success\n");
    assert!(exchange.handler().completed.load(Ordering::SeqCst));
    let r = c.get("type=catalog&mode=import&filename=import.xml").await;
    assert_eq!(r.lines()[0], "failure");
}

#[tokio::test]
async fn catalog_exchange_with_zip() {
    let (mut c, exchange) = Client::new(ExchangeConfig::default().file_limit(1000));
    c.login("catalog").await;
    assert_eq!(c.get("type=catalog&mode=init").await.lines()[0], "zip=yes");
    let archive = zip(&[
        ("import.xml", &fixture("import_2_10.xml")),
        ("offers.xml", &fixture("offers_2_07.xml")),
        ("import_files/aa/p-chair_1.jpg", b"jpeg"),
    ]);
    for part in archive.chunks(1000) {
        let r = c
            .call("type=catalog&mode=file&filename=pack.zip", None, part.to_vec())
            .await;
        assert_eq!(r.text(), "success\n");
    }
    for name in ["import.xml", "offers.xml"] {
        let r = c.get(&format!("type=catalog&mode=import&filename={name}")).await;
        assert_eq!(r.text(), "success\n", "{name}");
    }
    assert_eq!(
        *exchange.handler().imported.lock().unwrap(),
        [("import.xml".to_owned(), 3), ("offers.xml".to_owned(), 3)]
    );
}

#[tokio::test]
async fn hostile_uploads_are_refused() {
    let (mut c, _) = Client::new(
        ExchangeConfig::default()
            .max_file_size(1000)
            .max_unzipped_size(50),
    );
    c.login("catalog").await;
    c.get("type=catalog&mode=init").await;
    for name in ["../../etc/passwd", "%2Fabs.xml", "C:%5Cwin.xml", ""] {
        let r = c
            .call(
                &format!("type=catalog&mode=file&filename={name}"),
                None,
                b"x".to_vec(),
            )
            .await;
        assert_eq!(r.lines()[0], "failure", "{name}");
    }
    let r = c
        .call("type=catalog&mode=file&filename=a.xml", None, vec![b'x'; 1001])
        .await;
    assert!(r.text().contains("maximum file size"), "{}", r.text());

    let slip = zip(&[("../evil.xml", b"x")]);
    c.call("type=catalog&mode=file&filename=slip.zip", None, slip)
        .await;
    let r = c.get("type=catalog&mode=import&filename=evil.xml").await;
    assert!(r.text().contains("unsafe path"), "{}", r.text());

    c.get("type=catalog&mode=init").await;
    let bomb = zip(&[("big.xml", &[b'x'; 80])]);
    c.call("type=catalog&mode=file&filename=bomb.zip", None, bomb)
        .await;
    let r = c.get("type=catalog&mode=import&filename=big.xml").await;
    assert!(r.text().contains("unpacked size exceeds"), "{}", r.text());

    let r = c.get("mode=import&filename=x.xml").await;
    assert!(r.text().contains("missing or unknown type"));
    let r = c.get("type=catalog").await;
    assert_eq!(r.lines(), ["failure", "missing mode"]);
    let r = c.get("type=catalog&mode=query").await;
    assert!(r.text().contains("not supported"));
}

#[tokio::test]
async fn sale_exchange() {
    let (mut c, exchange) = Client::new(
        ExchangeConfig::default()
            .orders_encoding(Encoding::WINDOWS_1251)
            .version("2.08".into()),
    );
    c.login("sale").await;
    c.get("type=sale&mode=init").await;

    let r = c.get("type=sale&mode=query").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        r.headers[header::CONTENT_TYPE],
        "application/xml; charset=windows-1251"
    );
    assert!(
        r.body
            .starts_with(b"<?xml version=\"1.0\" encoding=\"windows-1251\"?>")
    );
    let orders = commerceml::from_slice(&r.body).unwrap().info;
    assert_eq!(orders.schema_version, "2.08");
    assert_eq!(orders.generated_at.len(), 19, "{}", orders.generated_at);
    assert_eq!(orders.documents[0].items[0].product.name, "Синтетический стул");

    assert_eq!(c.get("type=sale&mode=success").await.text(), "success\n");
    assert!(exchange.handler().orders_received.load(Ordering::SeqCst));

    let r = c
        .call(
            "type=sale&mode=file&filename=orders-1.xml",
            None,
            fixture("orders_2_03.xml"),
        )
        .await;
    assert_eq!(r.text(), "success\n");
    assert_eq!(
        c.get("type=sale&mode=import&filename=orders-1.xml").await.text(),
        "success\n"
    );
    let archive = zip(&[("orders-2.xml", &fixture("documents_2_09_2_10.xml"))]);
    let r = c
        .call("type=sale&mode=file&filename=orders.zip", None, archive)
        .await;
    assert_eq!(r.text(), "success\n");
    assert_eq!(
        *exchange.handler().sale_files.lock().unwrap(),
        [("orders-1.xml".to_owned(), 1), ("orders-2.xml".to_owned(), 1)]
    );
}

#[tokio::test]
async fn sessions_expire() {
    let (mut c, _) = Client::new(ExchangeConfig::default().session_ttl(Duration::from_millis(1)));
    c.login("catalog").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let r = c.get("type=catalog&mode=init").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    c.login("catalog").await;
}

struct Minimal;

impl ExchangeHandler for Minimal {
    async fn authenticate(&self, _: &str, _: &str) -> bool {
        true
    }
}

#[tokio::test]
async fn defaults_of_a_minimal_handler() {
    let exchange = Exchange::new(Minimal, MemoryStorage::new(), ExchangeConfig::default());
    let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange);
    let mut c = Client { app, cookie: None };
    c.login("sale").await;
    let r = c.get("type=sale&mode=query").await;
    assert_eq!(commerceml::from_slice(&r.body).unwrap().info.documents.len(), 0);
    c.call("type=catalog&mode=file&filename=a.xml", None, b"<a/>".to_vec())
        .await;
    let r = c.get("type=catalog&mode=import&filename=a.xml").await;
    assert_eq!(
        r.lines(),
        ["failure", "catalog import is not supported by this site"]
    );
    let r = c
        .call("type=sale&mode=file&filename=o.xml", None, b"<a/>".to_vec())
        .await;
    assert_eq!(
        r.lines(),
        ["failure", "order import is not supported by this site"]
    );
    assert_eq!(
        c.get("type=sale&mode=success").await.text(),
        "success
"
    );
    assert_eq!(
        c.get("type=catalog&mode=deactivate").await.text(),
        "success
"
    );
    assert_eq!(
        c.get("type=sale&mode=complete").await.text(),
        "success
"
    );
}
