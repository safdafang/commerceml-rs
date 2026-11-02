//! Server side of the **1C:Enterprise ↔ website exchange protocol**
//! (`/1c_exchange.php?type=catalog&mode=checkauth` and friends), built on
//! [`commerceml`].
//!
//! 1C starts every exchange. This crate answers its requests:
//!
//! | `type` | `mode` | What happens |
//! |---|---|---|
//! | any | `checkauth` | HTTP Basic auth via [`ExchangeHandler::authenticate`]; a session cookie is issued |
//! | any | `init` | announces `zip`, `file_limit`, `sessid`, `version`; clears the session's files |
//! | any | `file` | stores the body, appending parts of large files; zip archives are kept |
//! | `catalog` | `import` | unpacks archives, then [`ExchangeHandler::import_catalog`] (repeated while it returns [`ImportStatus::Progress`]) |
//! | `catalog` | `deactivate` | [`ExchangeHandler::deactivate`] (Bitrix extension) |
//! | any | `complete` | [`ExchangeHandler::complete`], then the session's files are deleted |
//! | `sale` | `query` | [`ExchangeHandler::query_orders`], written as CommerceML |
//! | `sale` | `success` | [`ExchangeHandler::orders_received`] |
//! | `sale` | `file` | stored, then [`ExchangeHandler::import_orders`] |
//!
//! The protocol core ([`Exchange::handle`]) does not depend on a web
//! framework. The `axum` feature (on by default) adds
//! [`axum::router_at`]. Files go to a [`FileStorage`]: [`FsStorage`] on
//! disk or [`MemoryStorage`].
//!
//! Uploads are bounded by [`ExchangeConfig`] limits: request size, file
//! size, bytes per session, unpacked size and entry count of archives. File
//! names are cleaned against path traversal, archive entries against
//! zip-slip.
//!
//! ```
//! use commerceml_exchange::{
//!     Credentials, Exchange, ExchangeConfig, ExchangeHandler, MemoryStorage, Request,
//! };
//!
//! struct Shop(Credentials);
//!
//! impl ExchangeHandler for Shop {
//!     async fn authenticate(&self, login: &str, password: &str) -> bool {
//!         self.0.check(login, password)
//!     }
//! }
//!
//! # tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
//! let exchange = Exchange::new(Shop(Credentials::new("1c", "secret")), MemoryStorage::new(), ExchangeConfig::default());
//! let response = exchange
//!     .handle(Request {
//!         query: "type=catalog&mode=checkauth".into(),
//!         authorization: Some("Basic MWM6c2VjcmV0".into()), // 1c:secret
//!         ..Request::default()
//!     })
//!     .await;
//! assert!(response.text().starts_with("success\nCML_EXCHANGE\n"));
//! # });
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub mod axum;
mod handler;
mod protocol;
mod service;
pub mod storage;
mod unzip;

pub use handler::{
    Credentials, ExchangeHandler, HandlerError, ImportStatus, ImportedFile, SessionInfo, constant_time_eq,
};
pub use protocol::{ExchangeType, Mode, Query, Request, Response};
pub use service::{Exchange, ExchangeConfig};
pub use storage::{FileStorage, FsStorage, MemoryStorage};

/// The examples of the repository READMEs, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
#[doc = include_str!("../../../README.ru.md")]
#[doc = include_str!("../README.md")]
#[doc = include_str!("../../commerceml/README.md")]
pub struct ReadmeDoctests;
