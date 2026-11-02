//! A small recursive-descent layer over `quick_xml::Reader`.
//!
//! Every entity reader receives the start element of its node and must
//! consume input up to and including the matching end tag. The helpers here
//! guarantee that for text, skipped and captured elements.

use std::io::BufRead;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};

use crate::ParseOptions;
use crate::error::{Error, Position, Result, Warning, WarningKind};
use crate::model::RawElement;
use crate::xml::writer::is_xml_char;

/// A start tag: local name, attributes and whether it is self-closing.
#[derive(Debug, Clone)]
pub(crate) struct Elem {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) empty: bool,
    pub(crate) pos: u64,
    /// Characters not allowed in XML were removed from attribute values.
    pub(crate) stripped: bool,
}

impl Elem {
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) struct XmlReader<B: BufRead> {
    xml: quick_xml::Reader<B>,
    buf: Vec<u8>,
    path: String,
    path_lens: Vec<usize>,
    warnings: Vec<Warning>,
    suppressed: u64,
    opts: ParseOptions,
}

/// Local part of a possibly prefixed name.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

/// Attributes that only carry namespace or schema information.
fn is_namespace_attr(raw_key: &str) -> bool {
    raw_key == "xmlns" || raw_key.starts_with("xmlns:") || raw_key.starts_with("xsi:")
}

impl<B: BufRead> XmlReader<B> {
    pub(crate) fn new(input: B, opts: ParseOptions) -> Self {
        let mut xml = quick_xml::Reader::from_reader(input);
        let cfg = xml.config_mut();
        cfg.trim_text_start = false;
        cfg.trim_text_end = false;
        cfg.check_end_names = true;
        cfg.expand_empty_elements = false;
        Self {
            xml,
            buf: Vec::with_capacity(4096),
            path: String::new(),
            path_lens: Vec::new(),
            warnings: Vec::new(),
            suppressed: 0,
            opts,
        }
    }

    pub(crate) fn position(&self) -> u64 {
        self.xml.buffer_position()
    }

    pub(crate) fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }

    pub(crate) fn suppressed_warnings(&self) -> u64 {
        self.suppressed
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    fn push(&mut self, name: &str) {
        self.path_lens.push(self.path.len());
        if !self.path.is_empty() {
            self.path.push('/');
        }
        self.path.push_str(name);
    }

    fn pop(&mut self) {
        if let Some(len) = self.path_lens.pop() {
            self.path.truncate(len);
        }
    }

    fn xml_error(&self, e: &quick_xml::Error) -> Error {
        xml_error(&self.xml, e)
    }

    fn eof_error(&self) -> Error {
        Error::Xml {
            message: format!("unexpected end of input inside <{}>", self.path),
            position: Position::at(self.xml.buffer_position()),
        }
    }

    /// Warns if attribute values of `elem` lost invalid characters.
    fn checked(&mut self, elem: Elem) -> Result<Elem> {
        if elem.stripped {
            let child = elem.empty.then_some(elem.name.as_str());
            self.warn(WarningKind::InvalidCharacter, elem.pos, child)?;
        }
        Ok(elem)
    }

    fn invalid_tag_error(&self) -> Error {
        Error::Xml {
            message: "character not allowed in XML in a tag".into(),
            position: Position::at(self.xml.buffer_position()),
        }
    }

    /// Records a warning at `pos`; `child` is appended to the current path.
    /// In strict mode returns an error instead.
    pub(crate) fn warn(&mut self, kind: WarningKind, pos: u64, child: Option<&str>) -> Result<()> {
        let path = match child {
            Some(c) if self.path.is_empty() => c.to_owned(),
            Some(c) => format!("{}/{c}", self.path),
            None => self.path.clone(),
        };
        let warning = Warning {
            kind,
            path,
            position: Position::at(pos),
        };
        if self.opts.strict {
            return Err(Error::Strict(Box::new(warning)));
        }
        if self.warnings.len() < self.opts.max_warnings {
            self.warnings.push(warning);
        } else {
            self.suppressed += 1;
        }
        Ok(())
    }

    /// Reads up to the first element of the document (the root).
    pub(crate) fn root(&mut self) -> Result<Option<Elem>> {
        loop {
            self.buf.clear();
            let pos = self.xml.buffer_position();
            match self.xml.read_event_into(&mut self.buf) {
                Err(e) => return Err(self.xml_error(&e)),
                Ok(Event::Eof) => return Ok(None),
                Ok(Event::Start(s)) => {
                    let elem = make_elem(&self.xml, &s, false, pos)?;
                    self.push(&elem.name);
                    return self.checked(elem).map(Some);
                }
                Ok(Event::Empty(s)) => {
                    let elem = make_elem(&self.xml, &s, true, pos)?;
                    return self.checked(elem).map(Some);
                }
                Ok(_) => {}
            }
        }
    }

    /// The next child element of `parent`, or `None` after its end tag.
    pub(crate) fn next_child(&mut self, parent: &Elem) -> Result<Option<Elem>> {
        if parent.empty {
            return Ok(None);
        }
        loop {
            self.buf.clear();
            let pos = self.xml.buffer_position();
            let event = self.xml.read_event_into(&mut self.buf);
            match event {
                Err(e) => return Err(self.xml_error(&e)),
                Ok(Event::Start(s)) => {
                    let elem = make_elem(&self.xml, &s, false, pos)?;
                    self.push(&elem.name);
                    return self.checked(elem).map(Some);
                }
                Ok(Event::Empty(s)) => {
                    let elem = make_elem(&self.xml, &s, true, pos)?;
                    return self.checked(elem).map(Some);
                }
                Ok(Event::End(_)) => {
                    self.pop();
                    return Ok(None);
                }
                Ok(Event::Text(t)) => {
                    if !t.chars().all(char::is_whitespace) {
                        self.warn(WarningKind::UnexpectedText, pos, None)?;
                    }
                }
                Ok(Event::CData(_) | Event::GeneralRef(_)) => {
                    self.warn(WarningKind::UnexpectedText, pos, None)?;
                }
                Ok(Event::Eof) => return Err(self.eof_error()),
                Ok(_) => {}
            }
        }
    }

    /// Text content of `elem`. Child elements are skipped with a warning.
    pub(crate) fn text(&mut self, elem: &Elem) -> Result<String> {
        let mut out = String::new();
        self.mixed(elem, &mut out, |r, child| {
            r.warn(
                WarningKind::UnexpectedChildren {
                    name: elem.name.clone(),
                },
                child.pos,
                None,
            )?;
            r.skip(&child)
        })?;
        Ok(out)
    }

    /// Reads mixed content: text goes to `out`, child elements to `on_child`,
    /// which must consume them.
    pub(crate) fn mixed(
        &mut self,
        elem: &Elem,
        out: &mut String,
        mut on_child: impl FnMut(&mut Self, Elem) -> Result<()>,
    ) -> Result<()> {
        if elem.empty {
            return Ok(());
        }
        loop {
            self.buf.clear();
            let pos = self.xml.buffer_position();
            let event = self.xml.read_event_into(&mut self.buf);
            match event {
                Err(e) => return Err(self.xml_error(&e)),
                Ok(Event::Text(t)) => {
                    if push_valid(out, &t.xml10_content()) {
                        self.warn(WarningKind::InvalidCharacter, pos, None)?;
                    }
                }
                Ok(Event::CData(c)) => {
                    if push_valid(out, &c.xml10_content()) {
                        self.warn(WarningKind::InvalidCharacter, pos, None)?;
                    }
                }
                Ok(Event::GeneralRef(r)) => {
                    if let Some(ch) = r.resolve_char_ref().map_err(|e| xml_error(&self.xml, &e))? {
                        if is_xml_char(ch) {
                            out.push(ch);
                        } else {
                            self.warn(WarningKind::InvalidCharacter, pos, None)?;
                        }
                    } else {
                        let name: &str = &r;
                        if let Some(v) = resolve_predefined_entity(name) {
                            out.push_str(v);
                        } else {
                            let name = name.to_owned();
                            out.push('&');
                            out.push_str(&name);
                            out.push(';');
                            self.warn(WarningKind::UnknownEntity { name }, pos, None)?;
                        }
                    }
                }
                Ok(Event::Start(s)) => {
                    let child = make_elem(&self.xml, &s, false, pos)?;
                    self.push(&child.name);
                    let child = self.checked(child)?;
                    on_child(self, child)?;
                }
                Ok(Event::Empty(s)) => {
                    let child = make_elem(&self.xml, &s, true, pos)?;
                    let child = self.checked(child)?;
                    on_child(self, child)?;
                }
                Ok(Event::End(_)) => {
                    self.pop();
                    return Ok(());
                }
                Ok(Event::Eof) => return Err(self.eof_error()),
                Ok(_) => {}
            }
        }
    }

    /// Consumes `elem` and everything inside it.
    pub(crate) fn skip(&mut self, elem: &Elem) -> Result<()> {
        self.consume(elem, None)
    }

    /// Consumes `elem`, re-serializing it verbatim. Returns `None` if it
    /// exceeded [`ParseOptions::max_raw_size`].
    pub(crate) fn capture(&mut self, elem: &Elem, start: &str) -> Result<Option<RawElement>> {
        let mut xml = String::with_capacity(64);
        xml.push('<');
        xml.push_str(start);
        if elem.empty {
            xml.push_str("/>");
        } else {
            xml.push('>');
            self.consume(elem, Some(&mut xml))?;
        }
        if xml.len() > self.opts.max_raw_size {
            return Ok(None);
        }
        Ok(Some(RawElement {
            name: elem.name.clone(),
            xml,
        }))
    }

    fn consume(&mut self, elem: &Elem, mut out: Option<&mut String>) -> Result<()> {
        if elem.empty {
            return Ok(());
        }
        let limit = self.opts.max_raw_size;
        let mut depth = 1usize;
        loop {
            self.buf.clear();
            let event = self.xml.read_event_into(&mut self.buf);
            let event = match event {
                Err(e) => return Err(self.xml_error(&e)),
                Ok(Event::Eof) => return Err(self.eof_error()),
                Ok(ev) => ev,
            };
            if let Event::Start(m) | Event::Empty(m) = &event
                && !is_valid_text(m)
            {
                return Err(self.invalid_tag_error());
            }
            if let Some(o) = out.as_deref_mut().filter(|o| o.len() <= limit) {
                match &event {
                    Event::Start(s) => {
                        o.push('<');
                        o.push_str(s);
                        o.push('>');
                    }
                    Event::Empty(s) => {
                        o.push('<');
                        o.push_str(s);
                        o.push_str("/>");
                    }
                    Event::End(e) => {
                        o.push_str("</");
                        o.push_str(if depth == 1 { &elem.name } else { e });
                        o.push('>');
                    }
                    Event::Text(t) => {
                        push_valid(o, t);
                    }
                    Event::GeneralRef(r) => {
                        o.push('&');
                        o.push_str(r);
                        o.push(';');
                    }
                    Event::CData(c) => {
                        o.push_str("<![CDATA[");
                        push_valid(o, c);
                        o.push_str("]]>");
                    }
                    Event::Comment(c) => {
                        o.push_str("<!--");
                        push_valid(o, c);
                        o.push_str("-->");
                    }
                    _ => {}
                }
            }
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        self.pop();
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    /// Handles an element unknown at this place: warns, then keeps it in
    /// `extra` (if given and allowed by the options) or skips it.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn unknown(&mut self, elem: Elem, extra: Option<&mut Vec<RawElement>>) -> Result<()> {
        let child = elem.empty.then_some(elem.name.as_str());
        self.warn(
            WarningKind::UnknownElement {
                name: elem.name.clone(),
            },
            elem.pos,
            child,
        )?;
        match extra {
            Some(list) if self.opts.keep_unknown => {
                let start = render_start(&elem);
                if let Some(raw) = self.capture(&elem, &start)? {
                    list.push(raw);
                }
                Ok(())
            }
            _ => self.skip(&elem),
        }
    }

    /// Warns about attributes of `elem` other than `known`.
    pub(crate) fn check_attrs(&mut self, elem: &Elem, known: &[&str]) -> Result<()> {
        for (k, _) in &elem.attrs {
            if !known.contains(&k.as_str()) {
                let pos = elem.pos;
                let child = elem.empty.then_some(elem.name.as_str());
                self.warn(WarningKind::UnknownAttribute { name: k.clone() }, pos, child)?;
            }
        }
        Ok(())
    }
}

fn xml_error<B>(xml: &quick_xml::Reader<B>, e: &quick_xml::Error) -> Error {
    Error::Xml {
        message: e.to_string(),
        position: Position::at(xml.error_position()),
    }
}

fn make_elem<B>(xml: &quick_xml::Reader<B>, s: &BytesStart<'_>, empty: bool, pos: u64) -> Result<Elem> {
    let invalid = || Error::Xml {
        message: "character not allowed in XML in a tag".into(),
        position: Position::at(pos),
    };
    let raw_name = s.name();
    if !is_valid_text(raw_name.as_ref()) {
        return Err(invalid());
    }
    let name = local(raw_name.as_ref()).to_owned();
    let mut attrs = Vec::new();
    let mut stripped = false;
    for a in s.attributes() {
        let a = a.map_err(|e| xml_error(xml, &quick_xml::Error::from(e)))?;
        let key = a.key.as_ref();
        if !is_valid_text(key) {
            return Err(invalid());
        }
        if is_namespace_attr(key) {
            continue;
        }
        let value = a
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| xml_error(xml, &e))?;
        let value = if is_valid_text(&value) {
            value.into_owned()
        } else {
            stripped = true;
            value.chars().filter(|&c| is_xml_char(c)).collect()
        };
        attrs.push((local(key).to_owned(), value));
    }
    Ok(Elem {
        name,
        attrs,
        empty,
        pos,
        stripped,
    })
}

/// Whether `s` has only characters allowed in XML 1.0. Fast for the usual
/// case: control characters are single bytes below 0x20, and U+FFFE/U+FFFF
/// are the only other exclusions possible in valid UTF-8.
fn is_valid_text(s: &str) -> bool {
    s.bytes().all(|b| b >= 0x20 || matches!(b, b'\t' | b'\n' | b'\r'))
        && !s.contains(['\u{FFFE}', '\u{FFFF}'])
}

/// Appends the XML-valid characters of `s` to `out`; returns whether any
/// were dropped.
fn push_valid(out: &mut String, s: &str) -> bool {
    if is_valid_text(s) {
        out.push_str(s);
        false
    } else {
        out.extend(s.chars().filter(|&c| is_xml_char(c)));
        true
    }
}

/// Rebuilds a start tag body (`name a="v"`) from a parsed element.
fn render_start(elem: &Elem) -> String {
    let mut s = elem.name.clone();
    for (k, v) in &elem.attrs {
        s.push(' ');
        s.push_str(k);
        s.push_str("=\"");
        crate::xml::writer::escape_into(&mut s, v, true);
        s.push('"');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(xml: &str) -> XmlReader<&[u8]> {
        XmlReader::new(xml.as_bytes(), ParseOptions::default())
    }

    #[test]
    fn text_with_entities_and_cdata() {
        let mut r = reader("<a>x &amp; &#1046;&#x416; <![CDATA[<y>]]>&nbsp;</a>");
        let root = r.root().unwrap().unwrap();
        let t = r.text(&root).unwrap();
        assert_eq!(t, "x & ЖЖ <y>&nbsp;");
        let w = r.take_warnings();
        assert_eq!(w.len(), 1);
        assert!(matches!(&w[0].kind, WarningKind::UnknownEntity { name } if name == "nbsp"));
    }

    #[test]
    fn children_and_capture() {
        let mut r =
            reader("<r xmlns='urn:x' xmlns:a='b' a:k='1'> <a:x k=\"&quot;\"><y>1</y><z/></a:x>t<q/></r>");
        let root = r.root().unwrap().unwrap();
        assert_eq!(root.attrs, vec![("k".to_owned(), "1".to_owned())]);
        let x = r.next_child(&root).unwrap().unwrap();
        assert_eq!((x.name.as_str(), x.attr("k")), ("x", Some("\"")));
        assert_eq!(r.path(), "r/x");
        let mut extra = Vec::new();
        r.unknown(x, Some(&mut extra)).unwrap();
        assert_eq!(extra[0].xml, "<x k=\"&quot;\"><y>1</y><z/></x>");
        let q = r.next_child(&root).unwrap().unwrap();
        assert!(q.empty);
        r.unknown(q, None).unwrap();
        assert!(r.next_child(&root).unwrap().is_none());
        let warnings = r.take_warnings();
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert_eq!(warnings[0].path, "r/x");
        assert!(matches!(warnings[1].kind, WarningKind::UnexpectedText));
        assert_eq!(warnings[2].path, "r/q");
    }

    #[test]
    fn strict_mode_turns_warnings_into_errors() {
        let opts = ParseOptions::default().strict(true);
        let mut r = XmlReader::new("<r><u/></r>".as_bytes(), opts);
        let root = r.root().unwrap().unwrap();
        let u = r.next_child(&root).unwrap().unwrap();
        assert!(matches!(r.unknown(u, None), Err(Error::Strict(_))));
    }

    #[test]
    fn eof_and_malformed() {
        let mut r = reader("<a><b>");
        let root = r.root().unwrap().unwrap();
        let b = r.next_child(&root).unwrap().unwrap();
        assert!(r.text(&b).is_err());
        let mut r = reader("<a></b>");
        let root = r.root().unwrap().unwrap();
        assert!(matches!(r.next_child(&root), Err(Error::Xml { .. })));
        let mut r = reader("  ");
        assert!(r.root().unwrap().is_none());
    }

    #[test]
    fn warning_cap() {
        let opts = ParseOptions::default().max_warnings(1);
        let mut r = XmlReader::new("<r><a/><b/><c/></r>".as_bytes(), opts);
        let root = r.root().unwrap().unwrap();
        while let Some(c) = r.next_child(&root).unwrap() {
            r.unknown(c, None).unwrap();
        }
        assert_eq!(r.take_warnings().len(), 1);
        assert_eq!(r.suppressed_warnings(), 2);
    }

    #[test]
    fn raw_size_limit() {
        let opts = ParseOptions::default().max_raw_size(10);
        let mut r = XmlReader::new("<r><big>0123456789abcdef</big></r>".as_bytes(), opts);
        let root = r.root().unwrap().unwrap();
        let big = r.next_child(&root).unwrap().unwrap();
        let mut extra = Vec::new();
        r.unknown(big, Some(&mut extra)).unwrap();
        assert_eq!(extra, []);
        assert!(r.next_child(&root).unwrap().is_none());
    }
}
