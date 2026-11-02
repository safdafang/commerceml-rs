//! The protocol state machine.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use bytes::Bytes;
use commerceml::{CommercialInfo, Encoding, WriteOptions};

use crate::handler::{ExchangeHandler, ImportStatus, ImportedFile, SessionInfo};
use crate::protocol::{ExchangeType, Mode, Query, Request, Response};
use crate::storage::{FileStorage, sanitize_name};
use crate::unzip;

/// Limits and protocol settings.
///
/// ```
/// use commerceml_exchange::ExchangeConfig;
///
/// let config = ExchangeConfig::default()
///     .zip(true)
///     .file_limit(50 * 1024 * 1024)
///     .orders_encoding(commerceml::Encoding::WINDOWS_1251);
/// # let _ = config;
/// ```
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    pub(crate) cookie_name: String,
    pub(crate) zip: bool,
    pub(crate) file_limit: u64,
    pub(crate) max_file_size: u64,
    pub(crate) max_session_bytes: u64,
    pub(crate) max_unzipped_size: u64,
    pub(crate) max_zip_entries: usize,
    pub(crate) session_ttl: Duration,
    pub(crate) version: String,
    pub(crate) orders_encoding: Encoding,
    pub(crate) sale_import_on_file: bool,
    pub(crate) cleanup_on_complete: bool,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            cookie_name: "CML_EXCHANGE".into(),
            zip: true,
            file_limit: 100 * 1024 * 1024,
            max_file_size: 4 * 1024 * 1024 * 1024,
            max_session_bytes: 8 * 1024 * 1024 * 1024,
            max_unzipped_size: 8 * 1024 * 1024 * 1024,
            max_zip_entries: 200_000,
            session_ttl: Duration::from_secs(3600),
            version: commerceml::DEFAULT_SCHEMA_VERSION.into(),
            orders_encoding: Encoding::UTF_8,
            sale_import_on_file: true,
            cleanup_on_complete: true,
        }
    }
}

macro_rules! setter {
    ($(#[$m:meta])* $name:ident: $ty:ty) => {
        $(#[$m])*
        #[must_use]
        pub fn $name(mut self, value: $ty) -> Self {
            self.$name = value;
            self
        }
    };
}

impl ExchangeConfig {
    setter!(
        /// Name of the session cookie (default `CML_EXCHANGE`).
        cookie_name: String
    );
    setter!(
        /// Accept zip archives (`zip=yes`, default `true`).
        zip: bool
    );
    setter!(
        /// Largest request body, announced as `file_limit` (default 100 MiB).
        /// 1C splits larger files into parts.
        file_limit: u64
    );
    setter!(
        /// Largest file after joining its parts (default 4 GiB).
        max_file_size: u64
    );
    setter!(
        /// Most bytes uploaded in one session (default 8 GiB).
        max_session_bytes: u64
    );
    setter!(
        /// Most bytes unpacked from the archives of one session (default 8 GiB).
        max_unzipped_size: u64
    );
    setter!(
        /// Most entries in one archive (default 200 000).
        max_zip_entries: usize
    );
    setter!(
        /// Sessions idle longer than this are dropped with their files
        /// (default 1 hour).
        session_ttl: Duration
    );
    setter!(
        /// CommerceML version announced in `init` and written into orders
        /// (default `2.10`).
        version: String
    );
    setter!(
        /// Encoding of the orders answered to `mode=query` (default UTF-8).
        orders_encoding: Encoding
    );
    setter!(
        /// Process a `type=sale` file right after `mode=file` (default
        /// `true`, as the 1C protocol description does not send
        /// `mode=import` for orders).
        sale_import_on_file: bool
    );
    setter!(
        /// Delete the session's files on `mode=complete` (default `true`).
        cleanup_on_complete: bool
    );

    /// The configured `file_limit`.
    pub fn get_file_limit(&self) -> u64 {
        self.file_limit
    }
}

#[derive(Debug)]
struct Session {
    info: SessionInfo,
    last_seen: Instant,
    uploaded: u64,
    unzipped: u64,
    /// Sale files already passed to the handler.
    processed: HashSet<String>,
}

struct Inner<H> {
    handler: H,
    storage: Arc<dyn FileStorage>,
    config: ExchangeConfig,
    sessions: Mutex<HashMap<String, Session>>,
}

/// An exchange endpoint: protocol logic around your [`ExchangeHandler`].
///
/// Cheap to clone. Feed it [`Request`]s with [`handle`](Self::handle), or
/// use the ready-made [axum router](crate::axum).
pub struct Exchange<H> {
    inner: Arc<Inner<H>>,
}

impl<H> Clone for Exchange<H> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `YYYY-MM-DDThh:mm:ss` in UTC for a Unix time.
pub(crate) fn format_datetime(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let secs = unix % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn new_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random number generator is unavailable");
    bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Decodes `Basic base64(login:password)`.
fn basic_credentials(header: &str) -> Option<(String, String)> {
    let (scheme, value) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()?;
    let text = match String::from_utf8(raw) {
        Ok(s) => s,
        Err(e) => encoding_rs::WINDOWS_1251
            .decode_without_bom_handling(e.as_bytes())
            .0
            .into_owned(),
    };
    let (login, password) = text.split_once(':')?;
    Some((login.to_owned(), password.to_owned()))
}

fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.trim())
}

/// Runs blocking storage work off the async threads.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(format!("internal error: {e}")))
}

impl<H: ExchangeHandler> Exchange<H> {
    /// An endpoint with your handler, a storage and settings.
    pub fn new(handler: H, storage: impl FileStorage, config: ExchangeConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                handler,
                storage: Arc::new(storage),
                config,
                sessions: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// The settings.
    pub fn config(&self) -> &ExchangeConfig {
        &self.inner.config
    }

    /// Your handler.
    pub fn handler(&self) -> &H {
        &self.inner.handler
    }

    fn sessions(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        self.inner.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answers one protocol request.
    pub async fn handle(&self, req: Request) -> Response {
        let q = Query::parse(&req.query);
        let Some(mode) = q.mode.clone() else {
            return Response::failure("missing mode");
        };
        if mode == Mode::CheckAuth {
            return self.checkauth(&req).await;
        }
        let Some(session) = self.session(&req, &q) else {
            return Response::unauthorized(
                "not authorized or the session expired; start with mode=checkauth",
            );
        };
        if mode == Mode::Init {
            return self.init(&session, &q).await;
        }
        let Some(kind) = q.exchange_type else {
            return Response::failure("missing or unknown type (catalog or sale)");
        };
        match (kind, mode) {
            (_, Mode::File) => self.file(kind, &session, &q, req.body).await,
            (ExchangeType::Catalog, Mode::Import) => self.import_catalog(&session, &q).await,
            (ExchangeType::Sale, Mode::Import) => match q.filename.as_deref().and_then(sanitize_name) {
                Some(name) if self.was_processed(&session.id, &name) => Response::success(),
                Some(name) => self.import_sale(&session, name).await,
                None => Response::failure("missing or invalid filename"),
            },
            (ExchangeType::Catalog, Mode::Deactivate) => respond(
                self.inner
                    .handler
                    .deactivate(session.clone(), q.timestamp.clone())
                    .await,
            ),
            (_, Mode::Complete) => {
                let result = self.inner.handler.complete(session.clone(), kind).await;
                if result.is_ok() && self.inner.config.cleanup_on_complete {
                    self.clear_files(&session.id).await;
                }
                respond(result)
            }
            (ExchangeType::Sale, Mode::Query) => self.query(&session).await,
            (ExchangeType::Sale, Mode::Success) => {
                respond(self.inner.handler.orders_received(session.clone()).await)
            }
            (_, m) => Response::failure(&format!("mode {m:?} is not supported for type={kind}")),
        }
    }

    async fn checkauth(&self, req: &Request) -> Response {
        let Some((login, password)) = req.authorization.as_deref().and_then(basic_credentials) else {
            return Response::unauthorized("HTTP Basic authentication required");
        };
        if !self.inner.handler.authenticate(&login, &password).await {
            return Response::unauthorized("wrong login or password");
        }
        self.expire_sessions().await;
        let token = new_token();
        let started_at = now_unix();
        let info = SessionInfo {
            id: token.clone(),
            login,
            started_at,
            version: self.inner.config.version.clone(),
        };
        self.sessions().insert(
            token.clone(),
            Session {
                info,
                last_seen: Instant::now(),
                uploaded: 0,
                unzipped: 0,
                processed: HashSet::new(),
            },
        );
        let cookie = &self.inner.config.cookie_name;
        let mut r = Response::lines([
            "success".to_owned(),
            cookie.clone(),
            token.clone(),
            format!("sessid={token}"),
            format!("timestamp={started_at}"),
        ]);
        r.headers.push((
            "Set-Cookie".into(),
            format!("{cookie}={token}; Path=/; HttpOnly; SameSite=Lax"),
        ));
        r
    }

    fn session(&self, req: &Request, q: &Query) -> Option<SessionInfo> {
        let from_cookie = req
            .cookie
            .as_deref()
            .and_then(|c| cookie_value(c, &self.inner.config.cookie_name));
        let token = from_cookie.or(q.sessid.as_deref())?;
        let ttl = self.inner.config.session_ttl;
        let mut sessions = self.sessions();
        let s = sessions.get_mut(token)?;
        if s.last_seen.elapsed() > ttl {
            return None;
        }
        s.last_seen = Instant::now();
        Some(s.info.clone())
    }

    async fn expire_sessions(&self) {
        let ttl = self.inner.config.session_ttl;
        let expired: Vec<String> = {
            let mut sessions = self.sessions();
            let expired: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| s.last_seen.elapsed() > ttl)
                .map(|(k, _)| k.clone())
                .collect();
            for k in &expired {
                sessions.remove(k);
            }
            expired
        };
        for id in expired {
            self.clear_files(&id).await;
        }
    }

    async fn clear_files(&self, session: &str) {
        let storage = Arc::clone(&self.inner.storage);
        let id = session.to_owned();
        let _ = blocking(move || storage.clear(&id).map_err(|e| e.to_string())).await;
    }

    async fn init(&self, session: &SessionInfo, q: &Query) -> Response {
        self.clear_files(&session.id).await;
        let version = match q.version.as_deref() {
            Some(v) if v.starts_with("2.") => v.to_owned(),
            _ => self.inner.config.version.clone(),
        };
        if let Some(s) = self.sessions().get_mut(&session.id) {
            s.uploaded = 0;
            s.unzipped = 0;
            s.processed.clear();
            s.info.version.clone_from(&version);
        }
        let c = &self.inner.config;
        Response::lines([
            format!("zip={}", if c.zip { "yes" } else { "no" }),
            format!("file_limit={}", c.file_limit),
            format!("sessid={}", session.id),
            format!("version={version}"),
        ])
    }

    async fn file(&self, kind: ExchangeType, session: &SessionInfo, q: &Query, body: Bytes) -> Response {
        let Some(name) = q.filename.as_deref().and_then(sanitize_name) else {
            return Response::failure("missing or invalid filename");
        };
        let c = &self.inner.config;
        let len = body.len() as u64;
        if len > c.file_limit {
            return Response::failure(&format!("request body exceeds file_limit={}", c.file_limit));
        }
        if !c.zip && is_zip(&name) {
            return Response::failure("zip archives are disabled (zip=no)");
        }
        {
            let mut sessions = self.sessions();
            let Some(s) = sessions.get_mut(&session.id) else {
                return Response::unauthorized("session expired");
            };
            if s.uploaded + len > c.max_session_bytes {
                return Response::failure("upload limit for this session exceeded");
            }
            s.uploaded += len;
            s.processed.remove(&name);
        }
        let storage = Arc::clone(&self.inner.storage);
        let (id, file_name, max) = (session.id.clone(), name.clone(), c.max_file_size);
        let stored = blocking(move || {
            let current = storage
                .size(&id, &file_name)
                .map_err(|e| e.to_string())?
                .unwrap_or(0);
            if current + body.len() as u64 > max {
                return Err(format!(
                    "{file_name} exceeds the maximum file size of {max} bytes"
                ));
            }
            storage.append(&id, &file_name, &body).map_err(|e| e.to_string())
        })
        .await;
        if let Err(e) = stored {
            return Response::failure(&e);
        }
        if kind == ExchangeType::Sale && c.sale_import_on_file {
            return self.import_sale(session, name).await;
        }
        Response::success()
    }

    /// Unpacks archives uploaded in this session, once.
    async fn unpack(&self, session: &SessionInfo) -> Result<Vec<String>, String> {
        let storage = Arc::clone(&self.inner.storage);
        let id = session.id.clone();
        let already = self.sessions().get(&id).map_or(0, |s| s.unzipped);
        let limits = unzip::Limits {
            max_total: self.inner.config.max_unzipped_size.saturating_sub(already),
            max_entries: self.inner.config.max_zip_entries,
        };
        let (names, bytes) = blocking(move || {
            let archives: Vec<String> = storage
                .list(&id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|n| is_zip(n))
                .collect();
            let mut names = Vec::new();
            let mut total = 0;
            for a in archives {
                let limits = unzip::Limits {
                    max_total: limits.max_total - total,
                    ..limits
                };
                let (mut n, bytes) = unzip::extract(storage.as_ref(), &id, &a, &limits)?;
                names.append(&mut n);
                total += bytes;
            }
            Ok((names, total))
        })
        .await?;
        if let Some(s) = self.sessions().get_mut(&session.id) {
            s.unzipped += bytes;
        }
        Ok(names)
    }

    async fn import_catalog(&self, session: &SessionInfo, q: &Query) -> Response {
        let Some(name) = q.filename.as_deref().and_then(sanitize_name) else {
            return Response::failure("missing or invalid filename");
        };
        if let Err(e) = self.unpack(session).await {
            return Response::failure(&e);
        }
        if !self.exists(&session.id, &name).await {
            return Response::failure(&format!("{name} was not uploaded"));
        }
        let file = ImportedFile::new(
            ExchangeType::Catalog,
            name,
            session.clone(),
            Arc::clone(&self.inner.storage),
        );
        match self.inner.handler.import_catalog(file).await {
            Ok(ImportStatus::Done) => Response::success(),
            Ok(ImportStatus::Progress(msg)) => Response::progress(&msg),
            Err(e) => Response::failure(&e.to_string()),
        }
    }

    async fn exists(&self, session: &str, name: &str) -> bool {
        let storage = Arc::clone(&self.inner.storage);
        let (id, n) = (session.to_owned(), name.to_owned());
        matches!(
            blocking(move || storage.size(&id, &n).map_err(|e| e.to_string())).await,
            Ok(Some(_))
        )
    }

    fn was_processed(&self, session: &str, name: &str) -> bool {
        self.sessions()
            .get(session)
            .is_some_and(|s| s.processed.contains(name))
    }

    /// Passes a sale file (or the XML files of a sale archive) to the handler.
    async fn import_sale(&self, session: &SessionInfo, name: String) -> Response {
        let names = if is_zip(&name) {
            match self.unpack(session).await {
                Ok(n) => n
                    .into_iter()
                    .filter(|n| n.to_ascii_lowercase().ends_with(".xml"))
                    .collect(),
                Err(e) => return Response::failure(&e),
            }
        } else {
            if !self.exists(&session.id, &name).await {
                return Response::failure(&format!("{name} was not uploaded"));
            }
            vec![name.clone()]
        };
        let mut progress = None;
        for n in names {
            let file = ImportedFile::new(
                ExchangeType::Sale,
                n.clone(),
                session.clone(),
                Arc::clone(&self.inner.storage),
            );
            match self.inner.handler.import_orders(file).await {
                Ok(ImportStatus::Done) => {
                    if let Some(s) = self.sessions().get_mut(&session.id) {
                        s.processed.insert(n);
                    }
                }
                Ok(ImportStatus::Progress(m)) => progress = Some(m),
                Err(e) => return Response::failure(&e.to_string()),
            }
        }
        if progress.is_none()
            && let Some(s) = self.sessions().get_mut(&session.id)
        {
            s.processed.insert(name);
        }
        match progress {
            Some(m) => Response::progress(&m),
            None => Response::success(),
        }
    }

    async fn query(&self, session: &SessionInfo) -> Response {
        let mut info: CommercialInfo = match self.inner.handler.query_orders(session.clone()).await {
            Ok(i) => i,
            Err(e) => return Response::failure(&e.to_string()),
        };
        if info.generated_at.is_empty() {
            info.generated_at = format_datetime(now_unix());
        }
        if info.schema_version.is_empty() || info.schema_version == commerceml::DEFAULT_SCHEMA_VERSION {
            info.schema_version.clone_from(&session.version);
        }
        let encoding = self.inner.config.orders_encoding;
        match WriteOptions::default().encoding(encoding).to_vec(&info) {
            Ok(xml) => Response {
                status: 200,
                content_type: format!("application/xml; charset={}", encoding.name()),
                headers: Vec::new(),
                body: Bytes::from(xml),
            },
            Err(e) => Response::failure(&format!("cannot write orders: {e}")),
        }
    }
}

fn is_zip(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

fn respond(result: Result<(), crate::HandlerError>) -> Response {
    match result {
        Ok(()) => Response::success(),
        Err(e) => Response::failure(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_formatting() {
        assert_eq!(format_datetime(0), "1970-01-01T00:00:00");
        assert_eq!(format_datetime(951_782_400), "2000-02-29T00:00:00");
        assert_eq!(format_datetime(1_793_620_800), "2026-11-02T12:00:00");
    }

    #[test]
    fn basic_auth_and_cookies() {
        let header = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("admin:p:w")
        );
        assert_eq!(basic_credentials(&header), Some(("admin".into(), "p:w".into())));
        let cp1251 = base64::engine::general_purpose::STANDARD.encode(b"\xE0\xE4\xEC:x");
        assert_eq!(
            basic_credentials(&format!("basic {cp1251}")),
            Some(("адм".into(), "x".into()))
        );
        assert_eq!(basic_credentials("Bearer abc"), None);
        assert_eq!(basic_credentials("Basic !!!"), None);
        assert_eq!(cookie_value("a=1; CML=tok ; b=2", "CML"), Some("tok"));
        assert_eq!(cookie_value("a=1", "CML"), None);
        assert_eq!(new_token().len(), 32);
        assert_ne!(new_token(), new_token());
    }
}
