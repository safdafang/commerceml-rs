//! `commerceml stats`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use commerceml::stream::{Event, StreamReader};
use commerceml::{Group, WarningKind};
use serde::Serialize;

use crate::output::human_size;
use crate::{Format, InputArgs};

#[derive(Debug, Args)]
pub(crate) struct StatsArgs {
    /// File to summarize.
    file: PathBuf,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    #[command(flatten)]
    input: InputArgs,
}

/// Counts gathered in one streaming pass.
#[derive(Debug, Default, Serialize)]
pub(crate) struct Stats {
    file: PathBuf,
    bytes: u64,
    encoding: String,
    schema_version: String,
    generated_at: String,
    seconds: f64,
    groups: usize,
    properties: usize,
    price_types: usize,
    warehouses: usize,
    units: usize,
    categories: usize,
    products: usize,
    products_deleted: usize,
    products_with_pictures: usize,
    offers: usize,
    offers_with_characteristic: usize,
    offers_in_stock: usize,
    offers_without_price: usize,
    prices: usize,
    offer_changes: usize,
    documents: usize,
    document_lines: usize,
    counterparties: usize,
    unknown_root_elements: usize,
    parser_warnings: u64,
    /// Unknown element names and how often they occur.
    unknown_elements: BTreeMap<String, usize>,
}

fn count_groups(g: &Group) -> usize {
    1 + g.groups.iter().map(count_groups).sum::<usize>()
}

pub(crate) fn collect(args: &StatsArgs) -> anyhow::Result<Stats> {
    let started = Instant::now();
    let mut s = Stats {
        file: args.file.clone(),
        bytes: std::fs::metadata(&args.file)?.len(),
        ..Stats::default()
    };
    let mut reader = StreamReader::open(&args.file, args.input.options()?)?;
    s.encoding = reader.encoding().to_string();
    for event in reader.by_ref() {
        match event? {
            Event::Start(h) => {
                s.schema_version = h.schema_version;
                s.generated_at = h.generated_at;
            }
            Event::Group(g) => s.groups += count_groups(&g),
            Event::Property(_) => s.properties += 1,
            Event::PriceType(_) => s.price_types += 1,
            Event::Warehouse(_) => s.warehouses += 1,
            Event::Unit(_) => s.units += 1,
            Event::Category(_) => s.categories += 1,
            Event::Product(p) => {
                s.products += 1;
                s.products_deleted += usize::from(p.is_deleted());
                s.products_with_pictures += usize::from(!p.pictures.is_empty());
            }
            Event::OfferPackageStart(p) => {
                s.price_types += p.price_types.len();
                s.warehouses += p.warehouses.len();
            }
            Event::Offer(o) => {
                s.offers += 1;
                s.prices += o.prices.len();
                s.offers_with_characteristic += usize::from(o.characteristic_id().is_some());
                s.offers_without_price += usize::from(o.prices.iter().all(|p| p.price_per_unit.is_none()));
                s.offers_in_stock += usize::from(
                    o.total_quantity()
                        .is_some_and(|q| !q.is_zero() && !q.is_negative()),
                );
            }
            Event::OfferChange(_) => s.offer_changes += 1,
            Event::Document(d) => {
                s.documents += 1;
                s.document_lines += d.items.len();
            }
            Event::Counterparty(_) => s.counterparties += 1,
            Event::Unknown(_) => s.unknown_root_elements += 1,
            _ => {}
        }
    }
    let warnings = reader.take_warnings();
    s.parser_warnings = warnings.len() as u64 + reader.suppressed_warnings();
    for w in warnings {
        if let WarningKind::UnknownElement { name } = w.kind {
            *s.unknown_elements.entry(name).or_default() += 1;
        }
    }
    s.seconds = started.elapsed().as_secs_f64();
    Ok(s)
}

#[allow(clippy::cast_precision_loss)]
fn print_text(s: &Stats) {
    println!("{}", s.file.display());
    println!(
        "  {}, {}, ВерсияСхемы {}, ДатаФормирования {}",
        human_size(s.bytes),
        s.encoding,
        s.schema_version,
        s.generated_at
    );
    let rows: [(&str, usize); 18] = [
        ("groups", s.groups),
        ("properties", s.properties),
        ("price types", s.price_types),
        ("warehouses", s.warehouses),
        ("units", s.units),
        ("categories", s.categories),
        ("products", s.products),
        ("  deleted", s.products_deleted),
        ("  with pictures", s.products_with_pictures),
        ("offers", s.offers),
        ("  variants (id#characteristic)", s.offers_with_characteristic),
        ("  in stock", s.offers_in_stock),
        ("  without price", s.offers_without_price),
        ("prices", s.prices),
        ("offer changes", s.offer_changes),
        ("documents", s.documents),
        ("  lines", s.document_lines),
        ("counterparties", s.counterparties),
    ];
    for (name, n) in rows.iter().filter(|(_, n)| *n > 0) {
        println!("  {name:<32}{n:>10}");
    }
    if s.parser_warnings > 0 {
        println!("  {:<32}{:>10}", "parser warnings", s.parser_warnings);
        for (name, n) in &s.unknown_elements {
            println!("    unknown <{name}>: {n}");
        }
    }
    let mb_s = s.bytes as f64 / 1e6 / s.seconds.max(1e-9);
    println!("  parsed in {:.2} s ({mb_s:.0} MB/s)", s.seconds);
}

pub(crate) fn run(args: &StatsArgs) -> anyhow::Result<()> {
    let s = collect(args)?;
    match args.format {
        Format::Text => print_text(&s),
        Format::Json => println!("{}", serde_json::to_string_pretty(&s)?),
    }
    Ok(())
}
