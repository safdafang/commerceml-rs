//! A minimal XML writer: escaping, indentation and output encoding.

use std::io::{self, Write};

use crate::Decimal;
use crate::encoding::Encoding;
use crate::model::RawElement;

/// Appends `s` to `out`, escaping XML special characters. In attributes,
/// whitespace characters that attribute normalization would change are
/// escaped too. Characters not allowed in XML 1.0 are dropped.
pub(crate) fn escape_into(out: &mut String, s: &str, attribute: bool) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' if attribute => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' if attribute => out.push_str("&#10;"),
            '\t' if attribute => out.push_str("&#9;"),
            '\t' | '\n' => out.push(c),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
}

/// Whether `c` may appear in an XML 1.0 document.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Where the writer's UTF-8 output goes: straight to `W`, or through an
/// encoder.
enum Sink<W: Write> {
    Utf8(W),
    Encoded {
        inner: W,
        encoder: encoding_rs::Encoder,
        buf: Vec<u8>,
    },
}

impl<W: Write> Sink<W> {
    fn write_str(&mut self, s: &str) -> io::Result<()> {
        match self {
            Self::Utf8(w) => w.write_all(s.as_bytes()),
            Self::Encoded { inner, encoder, buf } => {
                let mut rest = s;
                loop {
                    buf.clear();
                    buf.reserve(rest.len() * 2 + 16);
                    let (result, read, _) = encoder.encode_from_utf8_to_vec(rest, buf, false);
                    inner.write_all(buf)?;
                    rest = &rest[read..];
                    if matches!(result, encoding_rs::CoderResult::InputEmpty) {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn into_inner(self) -> W {
        match self {
            Self::Utf8(w) | Self::Encoded { inner: w, .. } => w,
        }
    }

    fn get_mut(&mut self) -> &mut W {
        match self {
            Self::Utf8(w) | Self::Encoded { inner: w, .. } => w,
        }
    }
}

/// Low-level XML output with optional pretty-printing.
pub(crate) struct XmlWriter<W: Write> {
    sink: Sink<W>,
    line: String,
    depth: usize,
    indent: Option<&'static str>,
}

/// Flush the line buffer to the sink once it grows past this size.
const FLUSH_AT: usize = 32 * 1024;

impl<W: Write> XmlWriter<W> {
    pub(crate) fn new(inner: W, encoding: Encoding, indent: bool) -> Self {
        let sink = if encoding.is_utf8() {
            Sink::Utf8(inner)
        } else {
            Sink::Encoded {
                inner,
                encoder: encoding.inner().new_encoder(),
                buf: Vec::new(),
            }
        };
        Self {
            sink,
            line: String::with_capacity(FLUSH_AT + 1024),
            depth: 0,
            indent: indent.then_some("\t"),
        }
    }

    pub(crate) fn declaration(&mut self, encoding: Encoding) -> io::Result<()> {
        self.line.push_str("<?xml version=\"1.0\" encoding=\"");
        self.line.push_str(encoding.name());
        self.line.push_str("\"?>");
        self.maybe_flush()
    }

    fn newline(&mut self) {
        if let Some(unit) = self.indent {
            self.line.push('\n');
            for _ in 0..self.depth {
                self.line.push_str(unit);
            }
        }
    }

    fn maybe_flush(&mut self) -> io::Result<()> {
        if self.line.len() >= FLUSH_AT {
            self.sink.write_str(&self.line)?;
            self.line.clear();
        }
        Ok(())
    }

    fn open_tag(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.newline();
        self.line.push('<');
        self.line.push_str(name);
        for (k, v) in attrs {
            self.line.push(' ');
            self.line.push_str(k);
            self.line.push_str("=\"");
            escape_into(&mut self.line, v, true);
            self.line.push('"');
        }
    }

    /// `<name a="v">` and one level deeper.
    pub(crate) fn start(&mut self, name: &str, attrs: &[(&str, &str)]) -> io::Result<()> {
        self.open_tag(name, attrs);
        self.line.push('>');
        self.depth += 1;
        self.maybe_flush()
    }

    /// `</name>` and one level up.
    pub(crate) fn end(&mut self, name: &str) -> io::Result<()> {
        self.depth = self.depth.saturating_sub(1);
        self.newline();
        self.line.push_str("</");
        self.line.push_str(name);
        self.line.push('>');
        self.maybe_flush()
    }

    /// `<name a="v"/>`.
    pub(crate) fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) -> io::Result<()> {
        self.open_tag(name, attrs);
        self.line.push_str("/>");
        self.maybe_flush()
    }

    /// `<name a="v">text</name>`, or `<name/>` for empty text.
    pub(crate) fn text_attrs(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) -> io::Result<()> {
        if text.is_empty() {
            return self.empty(name, attrs);
        }
        self.open_tag(name, attrs);
        self.line.push('>');
        escape_into(&mut self.line, text, false);
        self.line.push_str("</");
        self.line.push_str(name);
        self.line.push('>');
        self.maybe_flush()
    }

    /// `<name>text</name>`; always written.
    pub(crate) fn text(&mut self, name: &str, text: &str) -> io::Result<()> {
        self.text_attrs(name, &[], text)
    }

    /// `<name>text</name>` if `text` is not empty.
    pub(crate) fn text_nonempty(&mut self, name: &str, text: &str) -> io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.text(name, text)
    }

    /// `<name>text</name>` if present.
    pub(crate) fn opt(&mut self, name: &str, text: Option<&str>) -> io::Result<()> {
        match text {
            Some(t) => self.text(name, t),
            None => Ok(()),
        }
    }

    /// A decimal element if present.
    pub(crate) fn decimal(&mut self, name: &str, value: Option<Decimal>) -> io::Result<()> {
        match value {
            Some(d) => self.text(name, &d.to_string()),
            None => Ok(()),
        }
    }

    /// A boolean element (`true`/`false`) if present.
    pub(crate) fn boolean(&mut self, name: &str, value: Option<bool>) -> io::Result<()> {
        match value {
            Some(b) => self.text(name, if b { "true" } else { "false" }),
            None => Ok(()),
        }
    }

    /// Mixed content: text followed by child elements written by `children`.
    pub(crate) fn mixed(
        &mut self,
        name: &str,
        attrs: &[(&str, &str)],
        text: &str,
        has_children: bool,
        children: impl FnOnce(&mut Self) -> io::Result<()>,
    ) -> io::Result<()> {
        if !has_children {
            return self.text_attrs(name, attrs, text);
        }
        self.open_tag(name, attrs);
        self.line.push('>');
        escape_into(&mut self.line, text, false);
        self.depth += 1;
        children(self)?;
        self.end(name)
    }

    /// Writes kept unknown elements verbatim.
    pub(crate) fn raw(&mut self, extra: &[RawElement]) -> io::Result<()> {
        for r in extra {
            self.newline();
            self.line.extend(r.xml.chars().filter(|&c| is_xml_char(c)));
            self.maybe_flush()?;
        }
        Ok(())
    }

    /// Writes buffered output to the underlying writer.
    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.sink.write_str(&self.line)?;
        self.line.clear();
        self.sink.get_mut().flush()
    }

    /// Flushes and returns the underlying writer.
    pub(crate) fn finish(mut self) -> io::Result<W> {
        if self.indent.is_some() {
            self.line.push('\n');
        }
        self.flush()?;
        Ok(self.sink.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        let mut s = String::new();
        escape_into(&mut s, "a<b>&\"'\r\n\t\u{1}", false);
        assert_eq!(s, "a&lt;b&gt;&amp;\"'&#13;\n\t");
        s.clear();
        escape_into(&mut s, "\"\n\t", true);
        assert_eq!(s, "&quot;&#10;&#9;");
    }

    #[test]
    fn writes_indented_utf8() {
        let mut w = XmlWriter::new(Vec::new(), Encoding::UTF_8, true);
        w.declaration(Encoding::UTF_8).unwrap();
        w.start("a", &[("k", "v")]).unwrap();
        w.text("b", "x").unwrap();
        w.text("c", "").unwrap();
        w.opt("d", None).unwrap();
        w.boolean("e", Some(true)).unwrap();
        w.decimal("f", Some("1.50".parse().unwrap())).unwrap();
        w.mixed("g", &[], "t", true, |w| w.empty("h", &[])).unwrap();
        w.end("a").unwrap();
        let out = String::from_utf8(w.finish().unwrap()).unwrap();
        assert_eq!(
            out,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a k=\"v\">\n\t<b>x</b>\n\t<c/>\n\t<e>true</e>\n\t<f>1.50</f>\n\t<g>t\n\t\t<h/>\n\t</g>\n</a>\n"
        );
    }

    #[test]
    fn writes_windows_1251_with_char_refs() {
        let mut w = XmlWriter::new(Vec::new(), Encoding::WINDOWS_1251, false);
        w.text("Имя", "Ж€😀").unwrap();
        let out = w.finish().unwrap();
        let (decoded, _, _) = encoding_rs::WINDOWS_1251.decode(&out);
        assert_eq!(decoded, "<Имя>Ж€&#128512;</Имя>");
    }
}
