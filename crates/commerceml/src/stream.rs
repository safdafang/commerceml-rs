//! Streaming parser for large files.
//!
//! [`StreamReader`] yields [`Event`]s one entity at a time — a product, an
//! offer, a document — so memory use does not depend on the file size.
//!
//! ```
//! use commerceml::stream::{Event, StreamReader};
//!
//! let xml = r#"<КоммерческаяИнформация ВерсияСхемы="2.10" ДатаФормирования="2026-11-02T10:00:00">
//!   <Каталог><Ид>c</Ид><Наименование>Каталог</Наименование>
//!     <Товары>
//!       <Товар><Ид>1</Ид><Наименование>Первый</Наименование></Товар>
//!       <Товар><Ид>2</Ид><Наименование>Второй</Наименование></Товар>
//!     </Товары>
//!   </Каталог>
//! </КоммерческаяИнформация>"#;
//!
//! let mut names = Vec::new();
//! for event in StreamReader::new(xml.as_bytes())? {
//!     if let Event::Product(p) = event? {
//!         names.push(p.name);
//!     }
//! }
//! assert_eq!(names, ["Первый", "Второй"]);
//! # Ok::<(), commerceml::Error>(())
//! ```
//!
//! Section headers come twice: `…Start` before the first list item, with the
//! fields seen so far, and `…End` after the section, with all fields. List
//! fields of the header (`products`, `offers`, `groups`…) are always empty
//! there; the items arrive as separate events.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::ParseOptions;
use crate::encoding::{Encoding, Utf8Source};
use crate::error::{Error, Position, Result, Warning, WarningKind};
use crate::model::{
    Catalog, Category, Classifier, Counterparty, Document, Group, Offer, OfferChange, OfferPackage,
    OfferPackageChanges, PriceType, Product, Property, RawElement, Unit, Warehouse,
};
use crate::read;
use crate::xml::reader::{Elem, XmlReader};

/// Root element attributes: `ВерсияСхемы` and `ДатаФормирования`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Header {
    /// `ВерсияСхемы`.
    pub schema_version: String,
    /// `ДатаФормирования`.
    pub generated_at: String,
}

/// One item from a [`StreamReader`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "data", rename_all = "snake_case")
)]
#[non_exhaustive]
pub enum Event {
    /// The root element `КоммерческаяИнформация`; always first.
    Start(Header),
    /// `Классификатор` begins; lists are empty.
    ClassifierStart(Classifier),
    /// A top-level `Группа` (with its subgroups).
    Group(Group),
    /// A `Свойство`.
    Property(Property),
    /// A `ТипЦены`.
    PriceType(PriceType),
    /// A classifier `Склад`.
    Warehouse(Warehouse),
    /// A classifier `ЕдиницаИзмерения`.
    Unit(Unit),
    /// A classifier `Категория`.
    Category(Category),
    /// `Классификатор` ends; all header fields are set, lists are empty.
    ClassifierEnd(Classifier),
    /// `Каталог` begins; `products` is empty.
    CatalogStart(Catalog),
    /// A `Товар` of the catalog.
    Product(Product),
    /// `Каталог` ends; all header fields are set.
    CatalogEnd(Catalog),
    /// `ПакетПредложений` begins; `offers` is empty. Price types,
    /// warehouses and property values declared before the offers are set.
    OfferPackageStart(OfferPackage),
    /// A `Предложение`.
    Offer(Offer),
    /// `ПакетПредложений` ends.
    OfferPackageEnd(OfferPackage),
    /// `ИзмененияПакетаПредложений` begins.
    OfferPackageChangesStart(OfferPackageChanges),
    /// A changed `Предложение`.
    OfferChange(OfferChange),
    /// `ИзмененияПакетаПредложений` ends.
    OfferPackageChangesEnd(OfferPackageChanges),
    /// A `Документ` (also when wrapped in `Контейнер`).
    Document(Document),
    /// A root-level `Контрагенты/Контрагент`.
    Counterparty(Counterparty),
    /// A root-level element the library does not model.
    Unknown(RawElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Groups,
    Properties,
    PriceTypes,
    Warehouses,
    Units,
    Categories,
    Products,
    Offers,
    OfferChanges,
    Counterparties,
    Container,
}

impl ListKind {
    fn item(self) -> &'static str {
        match self {
            Self::Groups => "Группа",
            Self::Properties => "Свойство",
            Self::PriceTypes => "ТипЦены",
            Self::Warehouses => "Склад",
            Self::Units => "ЕдиницаИзмерения",
            Self::Categories => "Категория",
            Self::Products => "Товар",
            Self::Offers | Self::OfferChanges => "Предложение",
            Self::Counterparties => "Контрагент",
            Self::Container => "Документ",
        }
    }
}

enum Frame {
    Root(Elem),
    Classifier(Elem, Box<Classifier>, bool),
    Catalog(Elem, Box<Catalog>, bool),
    OfferPackage(Elem, Box<OfferPackage>, bool),
    Changes(Elem, Box<OfferPackageChanges>, bool),
    List(Elem, ListKind),
}

enum Action {
    Stay,
    Pop,
    Push(Frame),
}

/// Streaming CommerceML parser. See the [module documentation](self).
pub struct StreamReader<B: BufRead> {
    r: XmlReader<Utf8Source<B>>,
    encoding: Encoding,
    stack: Vec<Frame>,
    queue: VecDeque<Event>,
    started: bool,
    done: bool,
}

impl<R: Read> StreamReader<BufReader<R>> {
    /// Streams from any reader with default options.
    pub fn new(reader: R) -> Result<Self> {
        Self::with_options(reader, ParseOptions::default())
    }

    /// Streams from any reader.
    pub fn with_options(reader: R, options: ParseOptions) -> Result<Self> {
        Self::from_bufread(BufReader::with_capacity(64 * 1024, reader), options)
    }
}

impl StreamReader<BufReader<File>> {
    /// Opens a file for streaming.
    pub fn open(path: impl AsRef<Path>, options: ParseOptions) -> Result<Self> {
        Self::with_options(File::open(path)?, options)
    }
}

impl<B: BufRead> StreamReader<B> {
    /// Streams from a buffered reader.
    pub fn from_bufread(reader: B, options: ParseOptions) -> Result<Self> {
        let source = Utf8Source::new(reader, options.encoding)?;
        let encoding = source.encoding();
        Ok(Self {
            r: XmlReader::new(source, options),
            encoding,
            stack: Vec::new(),
            queue: VecDeque::new(),
            started: false,
            done: false,
        })
    }

    /// The input encoding (detected or forced).
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Takes the warnings collected so far.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        self.r.take_warnings()
    }

    /// Number of warnings dropped because of
    /// [`ParseOptions::max_warnings`].
    pub fn suppressed_warnings(&self) -> u64 {
        self.r.suppressed_warnings()
    }

    /// Current byte offset in the (UTF-8) input.
    pub fn position(&self) -> u64 {
        self.r.position()
    }

    fn start(&mut self) -> Result<()> {
        self.started = true;
        let Some(root) = self.r.root()? else {
            return Err(Error::Xml {
                message: "the document has no root element".into(),
                position: Position::at(self.r.position()),
            });
        };
        if root.name != "КоммерческаяИнформация" {
            return Err(Error::NotCommerceMl {
                found: root.name,
                position: Position::at(root.pos),
            });
        }
        self.r.check_attrs(&root, &["ВерсияСхемы", "ДатаФормирования"])?;
        for attr in ["ВерсияСхемы", "ДатаФормирования"] {
            if root.attr(attr).is_none() {
                self.r
                    .warn(WarningKind::MissingElement { name: attr.into() }, root.pos, None)?;
            }
        }
        self.queue.push_back(Event::Start(Header {
            schema_version: root.attr("ВерсияСхемы").unwrap_or_default().to_owned(),
            generated_at: root.attr("ДатаФормирования").unwrap_or_default().to_owned(),
        }));
        self.stack.push(Frame::Root(root));
        Ok(())
    }

    fn step(&mut self) -> Result<()> {
        if !self.started {
            return self.start();
        }
        let Some(frame) = self.stack.last_mut() else {
            self.done = true;
            return Ok(());
        };
        let r = &mut self.r;
        let queue = &mut self.queue;
        let action = match frame {
            Frame::Root(root) => match r.next_child(root)? {
                None => Action::Pop,
                Some(c) => match c.name.as_str() {
                    "Классификатор" => {
                        r.check_attrs(&c, &[])?;
                        Action::Push(Frame::Classifier(c, Box::default(), false))
                    }
                    "Каталог" => {
                        let changes_only = changes_only(r, &c)?;
                        let header = Catalog {
                            changes_only,
                            ..Catalog::default()
                        };
                        Action::Push(Frame::Catalog(c, Box::new(header), false))
                    }
                    "ПакетПредложений" => {
                        let changes_only = changes_only(r, &c)?;
                        let header = OfferPackage {
                            changes_only,
                            ..OfferPackage::default()
                        };
                        Action::Push(Frame::OfferPackage(c, Box::new(header), false))
                    }
                    "ИзмененияПакетаПредложений" => {
                        let changes_only = changes_only(r, &c)?;
                        let header = OfferPackageChanges {
                            changes_only,
                            ..OfferPackageChanges::default()
                        };
                        Action::Push(Frame::Changes(c, Box::new(header), false))
                    }
                    "Документ" => {
                        queue.push_back(Event::Document(read::document(r, &c)?));
                        Action::Stay
                    }
                    "Контрагенты" => {
                        let _ = changes_only(r, &c)?;
                        Action::Push(Frame::List(c, ListKind::Counterparties))
                    }
                    "Контейнер" => Action::Push(Frame::List(c, ListKind::Container)),
                    _ => {
                        let mut kept = Vec::new();
                        r.unknown(c, Some(&mut kept))?;
                        queue.extend(kept.into_iter().map(Event::Unknown));
                        Action::Stay
                    }
                },
            },
            Frame::Classifier(e, header, started) => match r.next_child(e)? {
                None => {
                    if !*started {
                        queue.push_back(Event::ClassifierStart((**header).clone()));
                    }
                    queue.push_back(Event::ClassifierEnd(std::mem::take(header)));
                    Action::Pop
                }
                Some(c) => {
                    let kind = match c.name.as_str() {
                        "Группы" => Some(ListKind::Groups),
                        "Свойства" => Some(ListKind::Properties),
                        "ТипыЦен" => Some(ListKind::PriceTypes),
                        "Склады" => Some(ListKind::Warehouses),
                        "ЕдиницыИзмерения" => Some(ListKind::Units),
                        "Категории" => Some(ListKind::Categories),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        if !*started {
                            *started = true;
                            queue.push_back(Event::ClassifierStart((**header).clone()));
                        }
                        Action::Push(Frame::List(c, kind))
                    } else {
                        if let Some(c) = read::classifier_field(r, c, header)? {
                            r.unknown(c, Some(&mut header.extra))?;
                        }
                        Action::Stay
                    }
                }
            },
            Frame::Catalog(e, header, started) => match r.next_child(e)? {
                None => {
                    if !*started {
                        queue.push_back(Event::CatalogStart((**header).clone()));
                    }
                    queue.push_back(Event::CatalogEnd(std::mem::take(header)));
                    Action::Pop
                }
                Some(c) if c.name == "Товары" => {
                    if !*started {
                        *started = true;
                        queue.push_back(Event::CatalogStart((**header).clone()));
                    }
                    Action::Push(Frame::List(c, ListKind::Products))
                }
                Some(c) => {
                    if let Some(c) = read::catalog_field(r, c, header)? {
                        r.unknown(c, Some(&mut header.extra))?;
                    }
                    Action::Stay
                }
            },
            Frame::OfferPackage(e, header, started) => match r.next_child(e)? {
                None => {
                    if !*started {
                        queue.push_back(Event::OfferPackageStart((**header).clone()));
                    }
                    queue.push_back(Event::OfferPackageEnd(std::mem::take(header)));
                    Action::Pop
                }
                Some(c) if c.name == "Предложения" => {
                    if !*started {
                        *started = true;
                        queue.push_back(Event::OfferPackageStart((**header).clone()));
                    }
                    Action::Push(Frame::List(c, ListKind::Offers))
                }
                Some(c) => {
                    if let Some(c) = read::offer_package_field(r, c, header)? {
                        r.unknown(c, Some(&mut header.extra))?;
                    }
                    Action::Stay
                }
            },
            Frame::Changes(e, header, started) => match r.next_child(e)? {
                None => {
                    if !*started {
                        queue.push_back(Event::OfferPackageChangesStart((**header).clone()));
                    }
                    queue.push_back(Event::OfferPackageChangesEnd(std::mem::take(header)));
                    Action::Pop
                }
                Some(c) if c.name == "Предложения" => {
                    if !*started {
                        *started = true;
                        queue.push_back(Event::OfferPackageChangesStart((**header).clone()));
                    }
                    Action::Push(Frame::List(c, ListKind::OfferChanges))
                }
                Some(c) => {
                    if let Some(c) = read::changes_field(r, c, header)? {
                        r.unknown(c, Some(&mut header.extra))?;
                    }
                    Action::Stay
                }
            },
            Frame::List(e, kind) => match r.next_child(e)? {
                None => Action::Pop,
                Some(c) if c.name == kind.item() => {
                    let event = match kind {
                        ListKind::Groups => Event::Group(read::group(r, &c)?),
                        ListKind::Properties => Event::Property(read::property(r, &c)?),
                        ListKind::PriceTypes => Event::PriceType(read::price_type(r, &c)?),
                        ListKind::Warehouses => Event::Warehouse(read::warehouse(r, &c)?),
                        ListKind::Units => Event::Unit(read::unit(r, &c)?),
                        ListKind::Categories => Event::Category(read::category(r, &c)?),
                        ListKind::Products => Event::Product(read::product(r, &c)?),
                        ListKind::Offers => Event::Offer(read::offer(r, &c)?),
                        ListKind::OfferChanges => Event::OfferChange(read::offer_change(r, &c)?),
                        ListKind::Counterparties => Event::Counterparty(read::counterparty(r, &c)?),
                        ListKind::Container => Event::Document(read::document(r, &c)?),
                    };
                    queue.push_back(event);
                    Action::Stay
                }
                Some(c) => {
                    r.unknown(c, None)?;
                    Action::Stay
                }
            },
        };
        match action {
            Action::Stay => {}
            Action::Pop => {
                self.stack.pop();
                if self.stack.is_empty() {
                    self.done = true;
                }
            }
            Action::Push(f) => self.stack.push(f),
        }
        Ok(())
    }
}

fn changes_only<B: BufRead>(r: &mut XmlReader<B>, e: &Elem) -> Result<Option<bool>> {
    r.check_attrs(e, &["СодержитТолькоИзменения"])?;
    match e.attr("СодержитТолькоИзменения") {
        Some(v) => read::parse_bool(r, "СодержитТолькоИзменения", v, e.pos),
        None => Ok(None),
    }
}

impl<B: BufRead> Iterator for StreamReader<B> {
    type Item = Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(e) = self.queue.pop_front() {
                return Some(Ok(e));
            }
            if self.done {
                return None;
            }
            if let Err(e) = self.step() {
                self.done = true;
                self.queue.clear();
                return Some(Err(e));
            }
        }
    }
}

impl<B: BufRead> std::iter::FusedIterator for StreamReader<B> {}
