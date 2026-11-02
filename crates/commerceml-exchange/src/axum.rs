//! A ready-made [axum](https://docs.rs/axum) router (feature `axum`).
//!
//! ```no_run
//! use commerceml_exchange::{Credentials, Exchange, ExchangeConfig, ExchangeHandler, FsStorage};
//!
//! struct Shop(Credentials);
//!
//! impl ExchangeHandler for Shop {
//!     async fn authenticate(&self, login: &str, password: &str) -> bool {
//!         self.0.check(login, password)
//!     }
//! }
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let exchange = Exchange::new(
//!     Shop(Credentials::new("1c", "secret")),
//!     FsStorage::new("exchange-data")?,
//!     ExchangeConfig::default(),
//! );
//! // 1C is configured with http://host:8080/1c_exchange.php
//! let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange);
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
//! axum::serve(listener, app).await?;
//! # Ok(())
//! # }
//! ```

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, RawQuery, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::any;

use crate::ExchangeHandler;
use crate::protocol::{Request, Response};
use crate::service::Exchange;

/// Extra room over `file_limit` for request overhead.
const BODY_SLACK: u64 = 64 * 1024;

/// A router serving the protocol at `path` (any HTTP method). The body size
/// limit follows [`ExchangeConfig::file_limit`](crate::ExchangeConfig::file_limit).
pub fn router_at<H: ExchangeHandler, S>(path: &str, exchange: Exchange<H>) -> Router<S> {
    let limit =
        usize::try_from(exchange.config().get_file_limit().saturating_add(BODY_SLACK)).unwrap_or(usize::MAX);
    Router::new()
        .route(path, any(serve::<H>))
        .layer(DefaultBodyLimit::max(limit))
        .with_state(exchange)
}

async fn serve<H: ExchangeHandler>(
    State(exchange): State<Exchange<H>>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let text = |name: header::HeaderName| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let request = Request {
        query: query.unwrap_or_default(),
        authorization: text(header::AUTHORIZATION),
        cookie: text(header::COOKIE),
        body,
    };
    into_axum(exchange.handle(request).await)
}

fn into_axum(r: Response) -> AxumResponse {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&r.content_type) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    for (k, v) in &r.headers {
        if let (Ok(k), Ok(v)) = (HeaderName::try_from(k.as_str()), HeaderValue::from_str(v)) {
            headers.append(k, v);
        }
    }
    let status = StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, headers, r.body).into_response()
}
