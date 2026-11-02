//! Parser and writer for **CommerceML 2**, the XML format 1C:Enterprise
//! uses to exchange catalogs, prices, stock and orders with websites.
//!
//! - Two ways to read: load a whole file into a [`CommercialInfo`]
//!   ([`from_path`], [`from_slice`], [`from_reader`]), or stream it entity by
//!   entity with constant memory ([`stream::StreamReader`]).
//! - UTF-8 and windows-1251 input, detected from the BOM or the XML
//!   declaration.
//! - Tolerant of versions 2.04–2.10 and of what real 1C configurations
//!   produce: elements the library does not know are reported as
//!   [`Warning`]s and kept as [`RawElement`]s, not errors.
//! - Writes everything it reads ([`to_string`], [`to_writer`],
//!   [`write::Writer`]); parse → write → parse gives the same data.
//!
//! # Quick start
//!
//! ```
//! let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
//! <КоммерческаяИнформация ВерсияСхемы="2.10" ДатаФормирования="2026-11-02T10:00:00">
//!   <Каталог>
//!     <Ид>catalog-1</Ид>
//!     <Наименование>Основной каталог</Наименование>
//!     <Товары>
//!       <Товар>
//!         <Ид>p-1</Ид>
//!         <Артикул>A-100</Артикул>
//!         <Наименование>Синтетический товар</Наименование>
//!         <БазоваяЕдиница Код="796" НаименованиеПолное="Штука">шт</БазоваяЕдиница>
//!       </Товар>
//!     </Товары>
//!   </Каталог>
//! </КоммерческаяИнформация>"#;
//!
//! let parsed = commerceml::from_str(xml)?;
//! let product = &parsed.info.products()[0];
//! assert_eq!(product.name, "Синтетический товар");
//! assert_eq!(product.article.as_deref(), Some("A-100"));
//! assert!(parsed.warnings.is_empty());
//!
//! // And back to XML.
//! let out = commerceml::to_string(&parsed.info)?;
//! assert_eq!(commerceml::from_str(&out)?.info, parsed.info);
//! # Ok::<(), commerceml::Error>(())
//! ```
//!
//! # Element names
//!
//! The API is in English. Every type and field documents the Russian
//! element it maps to. The main ones:
//!
//! | XML element | Rust |
//! |---|---|
//! | `КоммерческаяИнформация` | [`CommercialInfo`] |
//! | `Классификатор` | [`Classifier`] |
//! | `Группа` | [`Group`] |
//! | `Свойство` | [`Property`] |
//! | `ТипЦены` | [`PriceType`] |
//! | `Склад` | [`Warehouse`] |
//! | `Каталог` | [`Catalog`] |
//! | `Товар` | [`Product`] (in documents: [`DocumentItem`]) |
//! | `ПакетПредложений` | [`OfferPackage`] |
//! | `Предложение` | [`Offer`] |
//! | `Цена` | [`Price`] |
//! | `Остаток`, `Склад/@КоличествоНаСкладе` | [`Rest`], [`WarehouseStock`] |
//! | `ИзмененияПакетаПредложений` | [`OfferPackageChanges`] |
//! | `Документ` | [`Document`] |
//! | `Контрагент` | [`Counterparty`] |
//! | `ЗначениеРеквизита` | [`Requisite`] |
//!
//! # Features
//!
//! - `serde` — `Serialize`/`Deserialize` for the data model.
//! - `synth` — [`synth`], a generator of synthetic exports for tests and
//!   benchmarks.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod decimal;
mod encoding;
pub mod error;
mod model;
mod read;
pub mod stream;
#[cfg(feature = "synth")]
pub mod synth;
pub mod validate;
pub mod write;
mod xml;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub use decimal::{Decimal, ParseDecimalError};
pub use encoding::Encoding;
pub use error::{Error, Position, Result, Warning, WarningKind};
pub use model::*;
pub use write::WriteOptions;

use stream::{Event, StreamReader};

/// Parsing settings.
///
/// ```
/// use commerceml::{Encoding, ParseOptions};
///
/// let options = ParseOptions::default()
///     .strict(true)
///     .encoding(Some(Encoding::WINDOWS_1251));
/// # let _ = options;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    pub(crate) strict: bool,
    pub(crate) max_warnings: usize,
    pub(crate) keep_unknown: bool,
    pub(crate) max_raw_size: usize,
    pub(crate) encoding: Option<Encoding>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            strict: false,
            max_warnings: 10_000,
            keep_unknown: true,
            max_raw_size: 1024 * 1024,
            encoding: None,
        }
    }
}

impl ParseOptions {
    /// Turn every warning into [`Error::Strict`] (default `false`).
    #[must_use]
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Keep at most this many warnings; the rest are only counted
    /// (default 10 000).
    #[must_use]
    pub fn max_warnings(mut self, max: usize) -> Self {
        self.max_warnings = max;
        self
    }

    /// Keep unknown elements as [`RawElement`]s on the owning entity
    /// (default `true`). When `false` they are skipped.
    #[must_use]
    pub fn keep_unknown(mut self, keep: bool) -> Self {
        self.keep_unknown = keep;
        self
    }

    /// Unknown elements larger than this many bytes are skipped instead of
    /// kept (default 1 MiB). Bounds memory use on odd input.
    #[must_use]
    pub fn max_raw_size(mut self, bytes: usize) -> Self {
        self.max_raw_size = bytes;
        self
    }

    /// Force an input encoding instead of detecting it (default `None`).
    #[must_use]
    pub fn encoding(mut self, encoding: Option<Encoding>) -> Self {
        self.encoding = encoding;
        self
    }

    /// The forced input encoding, if any.
    pub fn forced_encoding(&self) -> Option<Encoding> {
        self.encoding
    }

    /// Parses a whole document from a reader.
    pub fn parse_reader<R: Read>(&self, reader: R) -> Result<Parsed> {
        collect(StreamReader::with_options(reader, self.clone())?)
    }

    /// Parses a whole document from bytes. Positions in errors and warnings
    /// get line and column numbers.
    pub fn parse_slice(&self, bytes: &[u8]) -> Result<Parsed> {
        let reader = StreamReader::from_bufread(bytes, self.clone());
        let result = reader.and_then(collect);
        let utf8 = || {
            encoding::decode_all(bytes, self.encoding)
                .map(|(s, _)| s)
                .unwrap_or_default()
        };
        match result {
            Ok(mut parsed) => {
                if !parsed.warnings.is_empty() {
                    let text = utf8();
                    for w in &mut parsed.warnings {
                        w.position.resolve(text.as_bytes());
                    }
                }
                Ok(parsed)
            }
            Err(mut e) => {
                if let Some(p) = e.position_mut() {
                    p.resolve(utf8().as_bytes());
                }
                Err(e)
            }
        }
    }

    /// Parses a whole document from a string.
    pub fn parse_str(&self, xml: &str) -> Result<Parsed> {
        self.parse_slice(xml.as_bytes())
    }

    /// Parses a whole file. Positions in errors and warnings get line and
    /// column numbers (the file is read a second time only if there are any).
    pub fn parse_path(&self, path: impl AsRef<Path>) -> Result<Parsed> {
        let path = path.as_ref();
        match self.parse_reader(File::open(path)?) {
            Ok(mut parsed) => {
                let mut positions: Vec<&mut Position> =
                    parsed.warnings.iter_mut().map(|w| &mut w.position).collect();
                resolve_in_file(path, self.encoding, &mut positions);
                Ok(parsed)
            }
            Err(mut e) => {
                if let Some(p) = e.position_mut() {
                    resolve_in_file(path, self.encoding, &mut [p]);
                }
                Err(e)
            }
        }
    }

    /// Opens a file for streaming.
    pub fn stream_path(&self, path: impl AsRef<Path>) -> Result<StreamReader<BufReader<File>>> {
        StreamReader::open(path, self.clone())
    }

    /// Streams from any reader.
    pub fn stream_reader<R: Read>(&self, reader: R) -> Result<StreamReader<BufReader<R>>> {
        StreamReader::with_options(reader, self.clone())
    }
}

/// Fills in line and column numbers by reading the file once more.
fn resolve_in_file(path: &Path, encoding: Option<Encoding>, positions: &mut [&mut Position]) {
    if positions.is_empty() {
        return;
    }
    let Ok(file) = File::open(path) else { return };
    let Ok(src) = encoding::Utf8Source::new(BufReader::new(file), encoding) else {
        return;
    };
    resolve_positions(src, positions);
}

/// Computes line/column for each position in one pass over a UTF-8 stream.
fn resolve_positions<B: std::io::BufRead>(mut src: B, positions: &mut [&mut Position]) {
    positions.sort_by_key(|p| p.offset);
    let (mut offset, mut line, mut column) = (0u64, 1u32, 1u32);
    let mut i = 0;
    while i < positions.len() {
        let Ok(buf) = src.fill_buf() else { break };
        if buf.is_empty() {
            break;
        }
        let mut consumed = 0;
        for &b in buf {
            while i < positions.len() && positions[i].offset == offset {
                positions[i].line = Some(line);
                positions[i].column = Some(column);
                i += 1;
            }
            if i == positions.len() {
                break;
            }
            if b == b'\n' {
                line += 1;
                column = 1;
            } else if b & 0xC0 != 0x80 {
                column += 1;
            }
            offset += 1;
            consumed += 1;
        }
        src.consume(consumed);
    }
    for p in &mut positions[i..] {
        p.line = Some(line);
        p.column = Some(column);
    }
}

/// A parsed document with the warnings found on the way.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Parsed {
    /// The data.
    pub info: CommercialInfo,
    /// Non-fatal problems, in input order.
    pub warnings: Vec<Warning>,
    /// Warnings dropped because of [`ParseOptions::max_warnings`].
    pub suppressed_warnings: u64,
    /// The input encoding.
    pub encoding: Encoding,
}

/// Assembles a whole document from a stream.
fn collect<B: std::io::BufRead>(mut stream: StreamReader<B>) -> Result<Parsed> {
    let mut info = CommercialInfo::default();
    let mut classifier = Classifier::default();
    let mut products = Vec::new();
    let mut offers = Vec::new();
    let mut changes = Vec::new();
    for event in stream.by_ref() {
        match event? {
            Event::Start(h) => {
                info.schema_version = h.schema_version;
                info.generated_at = h.generated_at;
            }
            Event::Group(g) => classifier.groups.push(g),
            Event::Property(p) => classifier.properties.push(p),
            Event::PriceType(p) => classifier.price_types.push(p),
            Event::Warehouse(w) => classifier.warehouses.push(w),
            Event::Unit(u) => classifier.units.push(u),
            Event::Category(c) => classifier.categories.push(c),
            Event::ClassifierEnd(mut c) => {
                let lists = std::mem::take(&mut classifier);
                c.groups = lists.groups;
                c.properties = lists.properties;
                c.price_types = lists.price_types;
                c.warehouses = lists.warehouses;
                c.units = lists.units;
                c.categories = lists.categories;
                info.classifier = Some(c);
            }
            Event::Product(p) => products.push(p),
            Event::CatalogEnd(mut c) => {
                c.products = std::mem::take(&mut products);
                info.catalog = Some(c);
            }
            Event::Offer(o) => offers.push(o),
            Event::OfferPackageEnd(mut p) => {
                p.offers = std::mem::take(&mut offers);
                info.offer_package = Some(p);
            }
            Event::OfferChange(o) => changes.push(o),
            Event::OfferPackageChangesEnd(mut c) => {
                c.offers = std::mem::take(&mut changes);
                info.offer_package_changes.push(c);
            }
            Event::Document(d) => info.documents.push(d),
            Event::Counterparty(c) => info.counterparties.push(c),
            Event::Unknown(raw) => info.extra.push(raw),
            Event::ClassifierStart(_)
            | Event::CatalogStart(_)
            | Event::OfferPackageStart(_)
            | Event::OfferPackageChangesStart(_) => {}
        }
    }
    Ok(Parsed {
        info,
        warnings: stream.take_warnings(),
        suppressed_warnings: stream.suppressed_warnings(),
        encoding: stream.encoding(),
    })
}

/// Parses a file with default options.
///
/// ```no_run
/// let parsed = commerceml::from_path("import.xml")?;
/// for w in &parsed.warnings {
///     eprintln!("warning: {w}");
/// }
/// println!("{} products", parsed.info.products().len());
/// # Ok::<(), commerceml::Error>(())
/// ```
pub fn from_path(path: impl AsRef<Path>) -> Result<Parsed> {
    ParseOptions::default().parse_path(path)
}

/// Parses bytes with default options.
pub fn from_slice(bytes: &[u8]) -> Result<Parsed> {
    ParseOptions::default().parse_slice(bytes)
}

/// Parses a string with default options.
pub fn from_str(xml: &str) -> Result<Parsed> {
    ParseOptions::default().parse_str(xml)
}

/// Parses from a reader with default options.
pub fn from_reader<R: Read>(reader: R) -> Result<Parsed> {
    ParseOptions::default().parse_reader(reader)
}

/// Serializes to a UTF-8 string with default options.
pub fn to_string(info: &CommercialInfo) -> Result<String> {
    let bytes = WriteOptions::default().to_vec(info)?;
    String::from_utf8(bytes).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

/// Serializes into a writer with default options (UTF-8, indented).
pub fn to_writer<W: std::io::Write>(info: &CommercialInfo, writer: W) -> Result<W> {
    WriteOptions::default().write(info, writer)
}

/// Serializes into a file with default options.
pub fn to_path(info: &CommercialInfo, path: impl AsRef<Path>) -> Result<()> {
    let file = std::io::BufWriter::new(File::create(path)?);
    to_writer(info, file)?
        .into_inner()
        .map_err(|e| Error::Io(e.into_error()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_many_positions_in_one_pass() {
        let text = "ab\nвг\n\nz";
        let mut ps = [
            Position::at(9),
            Position::at(0),
            Position::at(5),
            Position::at(100),
        ];
        let mut refs: Vec<&mut Position> = ps.iter_mut().collect();
        resolve_positions(text.as_bytes(), &mut refs);
        let got: Vec<_> = ps.iter().map(|p| (p.offset, p.line, p.column)).collect();
        assert_eq!(
            got,
            [
                (9, Some(4), Some(1)),
                (0, Some(1), Some(1)),
                (5, Some(2), Some(2)),
                (100, Some(4), Some(2))
            ]
        );
    }
}
