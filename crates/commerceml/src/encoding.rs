//! Character encodings of input and output files.
//!
//! The parser works on UTF-8 internally. Input in another encoding is
//! transcoded on the fly in fixed-size chunks, so memory use stays constant.

use std::fmt;
use std::io::{self, BufRead, Read};

use crate::error::{Error, Result};

/// A character encoding supported for reading and writing.
///
/// ```
/// use commerceml::Encoding;
///
/// assert_eq!(Encoding::for_label("cp1251"), Some(Encoding::WINDOWS_1251));
/// assert_eq!(Encoding::UTF_8.name(), "UTF-8");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Encoding(&'static encoding_rs::Encoding);

impl Encoding {
    /// UTF-8, the default.
    pub const UTF_8: Self = Self(encoding_rs::UTF_8);
    /// windows-1251 (Cyrillic), common in older 1C exports.
    pub const WINDOWS_1251: Self = Self(encoding_rs::WINDOWS_1251);

    /// Looks an encoding up by a label as used in XML declarations
    /// (`windows-1251`, `cp1251`, `utf-8`, `koi8-r`…). Case-insensitive.
    pub fn for_label(label: &str) -> Option<Self> {
        encoding_rs::Encoding::for_label(label.trim().as_bytes()).map(Self)
    }

    /// The canonical name, e.g. `windows-1251`.
    pub fn name(self) -> &'static str {
        self.0.name()
    }

    pub(crate) fn is_utf8(self) -> bool {
        self.0 == encoding_rs::UTF_8
    }

    pub(crate) fn inner(self) -> &'static encoding_rs::Encoding {
        self.0
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::UTF_8
    }
}

impl fmt::Debug for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Encoding({})", self.name())
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How many bytes are inspected to find the XML declaration.
const SNIFF_LEN: usize = 1024;

/// Finds the encoding from a BOM or the XML declaration at the start of
/// `prefix`. Returns the encoding and the BOM length to skip.
pub(crate) fn sniff(prefix: &[u8]) -> Result<(Encoding, usize)> {
    if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok((Encoding::UTF_8, 3));
    }
    if prefix.starts_with(&[0xFF, 0xFE]) {
        return Ok((Encoding(encoding_rs::UTF_16LE), 2));
    }
    if prefix.starts_with(&[0xFE, 0xFF]) {
        return Ok((Encoding(encoding_rs::UTF_16BE), 2));
    }
    if let Some(label) = declared_encoding(prefix) {
        return match Encoding::for_label(&label) {
            Some(e) if e.0 == encoding_rs::UTF_16LE || e.0 == encoding_rs::UTF_16BE => {
                Ok((Encoding::UTF_8, 0))
            }
            Some(e) => Ok((e, 0)),
            None => Err(Error::UnsupportedEncoding(label)),
        };
    }
    Ok((Encoding::UTF_8, 0))
}

/// Extracts `encoding="…"` from an `<?xml …?>` declaration.
fn declared_encoding(prefix: &[u8]) -> Option<String> {
    let start = prefix.iter().position(|b| !b.is_ascii_whitespace())?;
    let rest = &prefix[start..];
    if !rest.starts_with(b"<?xml") {
        return None;
    }
    let end = rest.windows(2).position(|w| w == b"?>")?;
    let decl = std::str::from_utf8(&rest[..end]).ok()?;
    let pos = decl.find("encoding")?;
    let after = decl[pos + "encoding".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &after[1..];
    let close = value.find(quote)?;
    Some(value[..close].to_owned())
}

/// A [`BufRead`] that yields UTF-8, transcoding from the source encoding
/// if necessary.
pub(crate) struct Utf8Source<R: BufRead> {
    inner: R,
    encoding: Encoding,
    decoder: Option<encoding_rs::Decoder>,
    out: Box<[u8]>,
    out_pos: usize,
    out_len: usize,
    eof: bool,
}

const OUT_BUF: usize = 64 * 1024;

impl<R: BufRead> Utf8Source<R> {
    /// Inspects the beginning of `inner`, skips a BOM and prepares to
    /// transcode. `forced` overrides detection.
    pub(crate) fn new(mut inner: R, forced: Option<Encoding>) -> Result<Self> {
        let prefix = fill_prefix(&mut inner)?;
        let (detected, bom) = sniff(&prefix)?;
        let encoding = forced.unwrap_or(detected);
        let skip = if forced.is_none_or(|f| f == detected) {
            bom
        } else {
            0
        };
        inner.consume(skip);
        let decoder = (!encoding.is_utf8()).then(|| encoding.0.new_decoder_without_bom_handling());
        Ok(Self {
            inner,
            encoding,
            decoder,
            out: vec![0; if encoding.is_utf8() { 0 } else { OUT_BUF }].into_boxed_slice(),
            out_pos: 0,
            out_len: 0,
            eof: false,
        })
    }

    /// The encoding of the source.
    pub(crate) fn encoding(&self) -> Encoding {
        self.encoding
    }
}

/// Reads up to [`SNIFF_LEN`] bytes into the reader's buffer without
/// consuming them, and returns a copy.
fn fill_prefix<R: BufRead>(inner: &mut R) -> io::Result<Vec<u8>> {
    let buf = inner.fill_buf()?;
    Ok(buf[..buf.len().min(SNIFF_LEN)].to_vec())
}

impl<R: BufRead> Read for Utf8Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<R: BufRead> BufRead for Utf8Source<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let Some(decoder) = self.decoder.as_mut() else {
            return self.inner.fill_buf();
        };
        while self.out_pos == self.out_len && !self.eof {
            let input = self.inner.fill_buf()?;
            let last = input.is_empty();
            let (_, read, written, _) = decoder.decode_to_utf8(input, &mut self.out, last);
            self.inner.consume(read);
            self.out_pos = 0;
            self.out_len = written;
            if last {
                self.eof = true;
            }
        }
        Ok(&self.out[self.out_pos..self.out_len])
    }

    fn consume(&mut self, amt: usize) {
        if self.decoder.is_some() {
            self.out_pos = (self.out_pos + amt).min(self.out_len);
        } else {
            self.inner.consume(amt);
        }
    }
}

/// Decodes a whole byte slice to UTF-8 using the detected encoding.
pub(crate) fn decode_all(bytes: &[u8], forced: Option<Encoding>) -> Result<(String, Encoding)> {
    let mut src = Utf8Source::new(bytes, forced)?;
    let mut out = String::new();
    let mut raw = Vec::new();
    src.read_to_end(&mut raw)?;
    out.push_str(&String::from_utf8_lossy(&raw));
    Ok((out, src.encoding()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_declaration_and_bom() {
        let (e, bom) = sniff(b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><a/>").unwrap();
        assert_eq!((e, bom), (Encoding::WINDOWS_1251, 0));
        let (e, bom) = sniff(b"\xEF\xBB\xBF<?xml version='1.0' encoding='windows-1251'?>").unwrap();
        assert_eq!((e, bom), (Encoding::UTF_8, 3));
        let (e, _) = sniff(b"  <?xml version = '1.0' encoding = 'UTF-8' ?>").unwrap();
        assert_eq!(e, Encoding::UTF_8);
        let (e, _) = sniff(b"<root/>").unwrap();
        assert_eq!(e, Encoding::UTF_8);
        let (e, _) = sniff(b"<?xml version='1.0' encoding='utf-16'?>").unwrap();
        assert_eq!(e, Encoding::UTF_8);
        assert!(matches!(
            sniff(b"<?xml version='1.0' encoding='x-unknown'?>"),
            Err(Error::UnsupportedEncoding(l)) if l == "x-unknown"
        ));
        assert_eq!(sniff(b"\xFF\xFE<\0").unwrap().1, 2);
    }

    #[test]
    fn transcodes_windows_1251() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1251
            .encode("<?xml version=\"1.0\" encoding=\"windows-1251\"?><Тег>Привет</Тег>");
        let mut src = Utf8Source::new(&bytes[..], None).unwrap();
        assert_eq!(src.encoding(), Encoding::WINDOWS_1251);
        let mut s = String::new();
        src.read_to_string(&mut s).unwrap();
        assert!(s.ends_with("<Тег>Привет</Тег>"));
    }

    #[test]
    fn transcodes_in_small_chunks() {
        let text = "<a>".to_owned() + &"Ж".repeat(200_000) + "</a>";
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(&text);
        let reader = io::BufReader::with_capacity(7, &bytes[..]);
        let mut src = Utf8Source::new(reader, Some(Encoding::WINDOWS_1251)).unwrap();
        let mut s = String::new();
        src.read_to_string(&mut s).unwrap();
        assert_eq!(s, text);
    }

    #[test]
    fn utf8_passthrough_skips_bom() {
        let mut src = Utf8Source::new(&b"\xEF\xBB\xBF<a/>"[..], None).unwrap();
        let mut s = String::new();
        src.read_to_string(&mut s).unwrap();
        assert_eq!(s, "<a/>");
        let (s, e) = decode_all(b"\xEF\xBB\xBF<b/>", None).unwrap();
        assert_eq!((s.as_str(), e), ("<b/>", Encoding::UTF_8));
    }

    #[test]
    fn labels() {
        assert_eq!(Encoding::for_label(" CP1251 "), Some(Encoding::WINDOWS_1251));
        assert_eq!(Encoding::for_label("nope"), None);
        assert_eq!(format!("{:?}", Encoding::default()), "Encoding(UTF-8)");
        assert_eq!(Encoding::WINDOWS_1251.to_string(), "windows-1251");
    }
}
