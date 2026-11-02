//! Errors and warnings.
//!
//! Parsing is lenient by default: anything that does not prevent reading the
//! rest of the file becomes a [`Warning`]. Only malformed XML, I/O failures
//! and a wrong root element are [`Error`]s. With
//! [`ParseOptions::strict`](crate::ParseOptions::strict) every warning is
//! turned into [`Error::Strict`].

use std::fmt;
use std::io;

/// A location in the parsed input.
///
/// `offset` is a byte offset into the input *after* transcoding to UTF-8
/// (equal to the file offset for UTF-8 files). `line` and `column` are
/// 1-based and filled in when the whole input is available, for example when
/// parsing from a slice or a file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    /// Byte offset in the UTF-8 input.
    pub offset: u64,
    /// 1-based line number, if known.
    pub line: Option<u32>,
    /// 1-based column (in characters), if known.
    pub column: Option<u32>,
}

impl Position {
    /// A position known only by its byte offset.
    pub const fn at(offset: u64) -> Self {
        Self {
            offset,
            line: None,
            column: None,
        }
    }

    /// Fills in `line` and `column` by scanning `utf8` (the decoded input)
    /// up to `offset`.
    #[allow(clippy::naive_bytecount)]
    pub fn resolve(&mut self, utf8: &[u8]) {
        let end = usize::try_from(self.offset).unwrap_or(usize::MAX).min(utf8.len());
        let before = &utf8[..end];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let line_start = before.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let column = String::from_utf8_lossy(&before[line_start..]).chars().count() + 1;
        self.line = u32::try_from(line).ok();
        self.column = u32::try_from(column).ok();
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "line {l}, column {c}"),
            _ => write!(f, "byte {}", self.offset),
        }
    }
}

/// What a [`Warning`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum WarningKind {
    /// An element the library does not know at this place. It was skipped,
    /// or kept as a [`RawElement`](crate::RawElement) on the owning entity.
    UnknownElement {
        /// Local name of the element.
        name: String,
    },
    /// An attribute the library does not know at this place.
    UnknownAttribute {
        /// Local name of the attribute.
        name: String,
    },
    /// A required element is missing; the field was left empty.
    MissingElement {
        /// Local name of the missing element.
        name: String,
    },
    /// A value could not be parsed (a number or a boolean); the field was
    /// left empty.
    InvalidValue {
        /// Element or attribute name.
        name: String,
        /// The offending text.
        value: String,
    },
    /// Text found where only elements are expected.
    UnexpectedText,
    /// Child elements found where only text is expected; they were skipped.
    UnexpectedChildren {
        /// Local name of the element that should hold only text.
        name: String,
    },
    /// An element that may occur once occurred again; the later one wins.
    Duplicate {
        /// Local name of the repeated element.
        name: String,
    },
    /// Characters not allowed in XML 1.0 (control characters, U+FFFE,
    /// U+FFFF) were removed from text or an attribute value.
    InvalidCharacter,
    /// An entity reference other than the five predefined XML entities.
    UnknownEntity {
        /// Entity name without `&` and `;`.
        name: String,
    },
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownElement { name } => write!(f, "unknown element <{name}>"),
            Self::UnknownAttribute { name } => write!(f, "unknown attribute `{name}`"),
            Self::MissingElement { name } => write!(f, "missing required element <{name}>"),
            Self::InvalidValue { name, value } => {
                write!(f, "invalid value {value:?} in <{name}>")
            }
            Self::UnexpectedText => f.write_str("unexpected text between elements"),
            Self::UnexpectedChildren { name } => {
                write!(f, "unexpected child elements in <{name}>")
            }
            Self::Duplicate { name } => write!(f, "repeated element <{name}>"),
            Self::InvalidCharacter => f.write_str("characters not allowed in XML were removed"),
            Self::UnknownEntity { name } => write!(f, "unknown entity &{name};"),
        }
    }
}

/// A non-fatal problem found while parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Warning {
    /// What happened.
    pub kind: WarningKind,
    /// Slash-separated element path, e.g. `КоммерческаяИнформация/Каталог/Товары/Товар`.
    pub path: String,
    /// Where it happened.
    pub position: Position,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {} ({})", self.kind, self.path, self.position)
    }
}

/// A fatal parsing or writing error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Reading or writing failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The input is not well-formed XML.
    #[error("malformed XML at {position}: {message}")]
    Xml {
        /// Description from the XML parser.
        message: String,
        /// Where the problem was found.
        position: Position,
    },
    /// The XML declaration names an encoding that is not supported.
    #[error("unsupported encoding `{0}`")]
    UnsupportedEncoding(String),
    /// The root element is not `КоммерческаяИнформация`.
    #[error("not a CommerceML document: root element is <{found}> at {position}")]
    NotCommerceMl {
        /// Local name of the root element found.
        found: String,
        /// Where it starts.
        position: Position,
    },
    /// A warning raised while parsing in strict mode.
    #[error("{0}")]
    Strict(Box<Warning>),
    /// The streaming [`Writer`](crate::write::Writer) was called out of order.
    #[error("writer misuse: {0}")]
    WriterState(&'static str),
}

impl Error {
    /// The position of the problem in the input, if the error has one.
    pub fn position(&self) -> Option<Position> {
        match self {
            Self::Xml { position, .. } | Self::NotCommerceMl { position, .. } => Some(*position),
            Self::Strict(w) => Some(w.position),
            _ => None,
        }
    }

    pub(crate) fn position_mut(&mut self) -> Option<&mut Position> {
        match self {
            Self::Xml { position, .. } | Self::NotCommerceMl { position, .. } => Some(position),
            Self::Strict(w) => Some(&mut w.position),
            _ => None,
        }
    }
}

/// Fills in `line` and `column` of each position by reading the file once
/// more. Use it for warnings from a [`StreamReader`](crate::stream::StreamReader),
/// which only knows byte offsets. Positions stay unchanged if the file
/// cannot be read.
pub fn resolve_positions_in_file(
    path: impl AsRef<std::path::Path>,
    encoding: Option<crate::Encoding>,
    positions: &mut [&mut Position],
) {
    crate::resolve_in_file(path.as_ref(), encoding, positions);
}

/// Result alias with [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_line_and_column() {
        let text = "ab\nвгд\nx".as_bytes();
        let mut p = Position::at(7);
        p.resolve(text);
        assert_eq!((p.line, p.column), (Some(2), Some(3)));
        assert_eq!(p.to_string(), "line 2, column 3");
        assert_eq!(Position::at(5).to_string(), "byte 5");
    }

    #[test]
    fn offset_past_end_is_clamped() {
        let mut p = Position::at(100);
        p.resolve(b"a\nb");
        assert_eq!(p.line, Some(2));
    }
}
