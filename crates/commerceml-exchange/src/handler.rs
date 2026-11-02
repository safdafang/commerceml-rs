//! The trait you implement: authentication and what to do with the data.

use std::fmt;
use std::future::Future;
use std::io::BufReader;
use std::sync::Arc;

use commerceml::stream::StreamReader;
use commerceml::{CommercialInfo, ParseOptions, Parsed};

use crate::protocol::ExchangeType;
use crate::storage::{FileStorage, ReadSeek};

/// An error returned by a handler; its message is sent to 1C after
/// `failure`.
pub type HandlerError = Box<dyn std::error::Error + Send + Sync>;

/// What is known about the current exchange session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The session token (also the cookie value).
    pub id: String,
    /// The login 1C authenticated with.
    pub login: String,
    /// Unix time of `checkauth`; Bitrix uses it as the `deactivate` boundary.
    pub started_at: u64,
    /// The CommerceML version agreed in `init`.
    pub version: String,
}

/// An uploaded file ready to be processed.
#[derive(Clone)]
pub struct ImportedFile {
    /// `catalog` or `sale`.
    pub exchange_type: ExchangeType,
    /// The cleaned file name, e.g. `import.xml` or `import_files/ab/x.jpg`.
    pub name: String,
    /// The session it belongs to.
    pub session: SessionInfo,
    storage: Arc<dyn FileStorage>,
}

impl fmt::Debug for ImportedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedFile")
            .field("exchange_type", &self.exchange_type)
            .field("name", &self.name)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl ImportedFile {
    pub(crate) fn new(
        exchange_type: ExchangeType,
        name: String,
        session: SessionInfo,
        storage: Arc<dyn FileStorage>,
    ) -> Self {
        Self {
            exchange_type,
            name,
            session,
            storage,
        }
    }

    /// Opens the file. Blocking: call it from `spawn_blocking`.
    pub fn open(&self) -> std::io::Result<Box<dyn ReadSeek>> {
        self.storage.open(&self.session.id, &self.name)
    }

    /// Whether the name ends with `.xml`.
    pub fn is_xml(&self) -> bool {
        std::path::Path::new(&self.name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
    }

    /// Streams the file as CommerceML. Blocking: call it from
    /// `spawn_blocking`.
    pub fn stream(
        &self,
        options: &ParseOptions,
    ) -> commerceml::Result<StreamReader<BufReader<Box<dyn ReadSeek>>>> {
        options.stream_reader(self.open()?)
    }

    /// Parses the whole file. Blocking: call it from `spawn_blocking`.
    pub fn parse(&self) -> commerceml::Result<Parsed> {
        commerceml::from_reader(self.open()?)
    }
}

/// The result of processing a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportStatus {
    /// Finished; answered with `success`.
    Done,
    /// Not finished; answered with `progress` and this message, and 1C
    /// sends the same `import` request again. Use it to split long imports
    /// into steps that fit into the HTTP timeout.
    Progress(String),
}

/// Business logic of an exchange endpoint.
///
/// Only [`authenticate`](Self::authenticate) is required; the other methods
/// have defaults that answer `success` (or `failure` for imports you do not
/// handle). Long-running work should go to `tokio::task::spawn_blocking`.
///
/// ```
/// use commerceml_exchange::{ExchangeHandler, ImportStatus, ImportedFile, HandlerError, Credentials};
///
/// struct Shop {
///     credentials: Credentials,
/// }
///
/// impl ExchangeHandler for Shop {
///     async fn authenticate(&self, login: &str, password: &str) -> bool {
///         self.credentials.check(login, password)
///     }
///
///     async fn import_catalog(&self, file: ImportedFile) -> Result<ImportStatus, HandlerError> {
///         if !file.is_xml() {
///             return Ok(ImportStatus::Done); // pictures are just stored
///         }
///         let products = tokio::task::spawn_blocking(move || {
///             file.parse().map(|p| p.info.products().len())
///         })
///         .await??;
///         println!("{products} products");
///         Ok(ImportStatus::Done)
///     }
/// }
/// ```
pub trait ExchangeHandler: Send + Sync + 'static {
    /// Checks the HTTP Basic credentials of `checkauth`.
    fn authenticate(&self, login: &str, password: &str) -> impl Future<Output = bool> + Send;

    /// `type=catalog&mode=import`: process an uploaded file (`import.xml`,
    /// `offers.xml`, pictures…).
    fn import_catalog(
        &self,
        file: ImportedFile,
    ) -> impl Future<Output = Result<ImportStatus, HandlerError>> + Send {
        let _ = file;
        async { Err("catalog import is not supported by this site".into()) }
    }

    /// `type=catalog&mode=deactivate` (Bitrix extension): deactivate objects
    /// not updated since `timestamp` (Unix time from `checkauth`).
    fn deactivate(
        &self,
        session: SessionInfo,
        timestamp: Option<String>,
    ) -> impl Future<Output = Result<(), HandlerError>> + Send {
        let _ = (session, timestamp);
        async { Ok(()) }
    }

    /// `mode=complete` (Bitrix extension): the exchange is over.
    fn complete(
        &self,
        session: SessionInfo,
        exchange_type: ExchangeType,
    ) -> impl Future<Output = Result<(), HandlerError>> + Send {
        let _ = (session, exchange_type);
        async { Ok(()) }
    }

    /// `type=sale&mode=query`: orders to send to 1C. Return documents in
    /// [`CommercialInfo::documents`]; an empty `generated_at` is filled in.
    fn query_orders(
        &self,
        session: SessionInfo,
    ) -> impl Future<Output = Result<CommercialInfo, HandlerError>> + Send {
        let _ = session;
        async { Ok(CommercialInfo::default()) }
    }

    /// `type=sale&mode=success`: 1C saved the orders of the last `query`;
    /// mark them as exported.
    fn orders_received(&self, session: SessionInfo) -> impl Future<Output = Result<(), HandlerError>> + Send {
        let _ = session;
        async { Ok(()) }
    }

    /// `type=sale&mode=file`: order changes from 1C (statuses, payments).
    fn import_orders(
        &self,
        file: ImportedFile,
    ) -> impl Future<Output = Result<ImportStatus, HandlerError>> + Send {
        let _ = file;
        async { Err("order import is not supported by this site".into()) }
    }
}

/// A fixed login and password, compared in constant time.
#[derive(Clone)]
pub struct Credentials {
    login: String,
    password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("login", &self.login)
            .field("password", &"***")
            .finish()
    }
}

impl Credentials {
    /// Credentials 1C must send.
    pub fn new(login: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            login: login.into(),
            password: password.into(),
        }
    }

    /// Whether `login` and `password` match.
    pub fn check(&self, login: &str, password: &str) -> bool {
        let l = constant_time_eq(self.login.as_bytes(), login.as_bytes());
        let p = constant_time_eq(self.password.as_bytes(), password.as_bytes());
        l & p
    }
}

/// Compares byte strings in time independent of where they differ.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = u8::from(a.len() != b.len());
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0xFF);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials() {
        let c = Credentials::new("admin", "secret");
        assert!(c.check("admin", "secret"));
        assert!(!c.check("admin", "secreT"));
        assert!(!c.check("admin", "secret2"));
        assert!(!c.check("", ""));
        assert!(!format!("{c:?}").contains("secret"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"a", b""));
    }
}
