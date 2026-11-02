//! Requests and responses of the exchange protocol, independent of any
//! HTTP framework.

use std::fmt;

use bytes::Bytes;

/// `type=` of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExchangeType {
    /// `type=catalog`: classifier, catalog, offers from 1C to the site.
    Catalog,
    /// `type=sale`: orders between the site and 1C.
    Sale,
}

impl ExchangeType {
    /// The value used in URLs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Sale => "sale",
        }
    }
}

impl fmt::Display for ExchangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `mode=` of a request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Authenticate with HTTP Basic auth and open a session.
    CheckAuth,
    /// Ask for exchange parameters (zip support, file size limit).
    Init,
    /// Upload a file or a part of it.
    File,
    /// Process an uploaded file; repeated while the answer is `progress`.
    Import,
    /// Deactivate objects not updated since a timestamp (Bitrix extension).
    Deactivate,
    /// The exchange is over (Bitrix extension).
    Complete,
    /// Download orders from the site.
    Query,
    /// The orders from the last `query` were saved in 1C.
    Success,
    /// Anything else.
    Other(String),
}

impl Mode {
    fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "checkauth" => Self::CheckAuth,
            "init" => Self::Init,
            "file" => Self::File,
            "import" => Self::Import,
            "deactivate" => Self::Deactivate,
            "complete" => Self::Complete,
            "query" => Self::Query,
            "success" => Self::Success,
            _ => Self::Other(s.to_owned()),
        }
    }
}

/// Decoded query parameters of a request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// `type`.
    pub exchange_type: Option<ExchangeType>,
    /// `mode`.
    pub mode: Option<Mode>,
    /// `filename`.
    pub filename: Option<String>,
    /// `version` — CommerceML version 1C asks for in `init`.
    pub version: Option<String>,
    /// `sessid` — session key, an alternative to the cookie.
    pub sessid: Option<String>,
    /// `timestamp` of `deactivate`.
    pub timestamp: Option<String>,
}

impl Query {
    /// Parses a raw query string such as `type=catalog&mode=file&filename=import.xml`.
    ///
    /// Percent-encoded bytes are decoded as UTF-8, or as windows-1251 when
    /// they are not valid UTF-8.
    pub fn parse(raw: &str) -> Self {
        let mut q = Self::default();
        for pair in raw.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode(v);
            match percent_decode(k).as_str() {
                "type" => {
                    q.exchange_type = match value.to_ascii_lowercase().as_str() {
                        "catalog" => Some(ExchangeType::Catalog),
                        "sale" => Some(ExchangeType::Sale),
                        _ => None,
                    }
                }
                "mode" => q.mode = Some(Mode::parse(&value)),
                "filename" => q.filename = Some(value),
                "version" => q.version = Some(value),
                "sessid" => q.sessid = Some(value),
                "timestamp" => q.timestamp = Some(value),
                _ => {}
            }
        }
        q
    }
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes `application/x-www-form-urlencoded` text. Bytes that are not
/// valid UTF-8 are read as windows-1251, which older 1C versions use.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push((h << 4) | l);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b => out.push(b),
        }
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(s) => s,
        Err(e) => encoding_rs::WINDOWS_1251
            .decode_without_bom_handling(e.as_bytes())
            .0
            .into_owned(),
    }
}

/// A request as the protocol core sees it.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// The raw query string (without `?`).
    pub query: String,
    /// Value of the `Authorization` header.
    pub authorization: Option<String>,
    /// Value of the `Cookie` header.
    pub cookie: Option<String>,
    /// The request body (file contents for `mode=file`).
    pub body: Bytes,
}

/// A response to send back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// `Content-Type`.
    pub content_type: String,
    /// Extra headers (`Set-Cookie`, `WWW-Authenticate`).
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Bytes,
}

const TEXT: &str = "text/plain; charset=utf-8";

impl Response {
    /// `200` with plain-text lines joined by `\n`.
    pub fn lines<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut body = String::new();
        for l in lines {
            body.push_str(l.as_ref());
            body.push('\n');
        }
        Self {
            status: 200,
            content_type: TEXT.to_owned(),
            headers: Vec::new(),
            body: Bytes::from(body),
        }
    }

    /// `success`.
    pub fn success() -> Self {
        Self::lines(["success"])
    }

    /// `progress` and a status line: 1C repeats the request.
    pub fn progress(message: &str) -> Self {
        Self::lines(["progress", &one_line(message)])
    }

    /// `failure` and the reason.
    pub fn failure(reason: &str) -> Self {
        Self::lines(["failure", &one_line(reason)])
    }

    /// `401` asking for Basic credentials.
    pub fn unauthorized(reason: &str) -> Self {
        let mut r = Self::failure(reason);
        r.status = 401;
        r.headers.push((
            "WWW-Authenticate".into(),
            "Basic realm=\"1C exchange\", charset=\"UTF-8\"".into(),
        ));
        r
    }

    /// The body as text (lossy), for tests and logs.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Protocol answers are line-based: keep messages on one line.
fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query() {
        let q = Query::parse(
            "type=catalog&mode=file&filename=import_files%2Fab%2Fx%20y.jpg&sessid=s1&version=3.1",
        );
        assert_eq!(q.exchange_type, Some(ExchangeType::Catalog));
        assert_eq!(q.mode, Some(Mode::File));
        assert_eq!(q.filename.as_deref(), Some("import_files/ab/x y.jpg"));
        assert_eq!(q.sessid.as_deref(), Some("s1"));
        assert_eq!(q.version.as_deref(), Some("3.1"));
        let q = Query::parse("type=SALE&mode=CheckAuth&junk&timestamp=1700000000");
        assert_eq!(q.exchange_type, Some(ExchangeType::Sale));
        assert_eq!(q.mode, Some(Mode::CheckAuth));
        assert_eq!(q.timestamp.as_deref(), Some("1700000000"));
        assert_eq!(Query::parse("mode=other").mode, Some(Mode::Other("other".into())));
        assert_eq!(Query::parse("type=x").exchange_type, None);
    }

    #[test]
    fn decodes_utf8_and_windows_1251_names() {
        assert_eq!(percent_decode("%D0%B8%D0%BC%D1%8F.xml"), "имя.xml");
        assert_eq!(percent_decode("%E8%EC%FF.xml"), "имя.xml");
        assert_eq!(percent_decode("a+b%2"), "a b%2");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn responses() {
        assert_eq!(Response::success().text(), "success\n");
        assert_eq!(Response::progress("step\n2").text(), "progress\nstep 2\n");
        let r = Response::unauthorized("no");
        assert_eq!((r.status, r.text().as_str()), (401, "failure\nno\n"));
        assert_eq!(ExchangeType::Sale.to_string(), "sale");
    }
}
