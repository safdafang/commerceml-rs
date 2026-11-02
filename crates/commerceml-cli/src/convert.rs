//! `commerceml convert`.

use std::io::Write;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use commerceml::stream::{Event, StreamReader};
use commerceml::{Document, Offer, Product, Role};

use crate::InputArgs;
use crate::output::{Csv, open_output, to_json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Target {
    /// The whole document as one JSON value (loads the file into memory).
    Json,
    /// One JSON object per line and entity, streamed.
    Jsonl,
    /// A flat table of one entity kind, streamed.
    Csv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Entity {
    /// Catalog products.
    Products,
    /// Offers; one row per offer and price type.
    Offers,
    /// Document lines; one row per line.
    Documents,
}

#[derive(Debug, Args)]
pub(crate) struct ConvertArgs {
    /// Input file.
    file: PathBuf,
    /// Output format.
    #[arg(long, value_enum)]
    to: Target,
    /// What to export as CSV (default: products, or offers for an offers
    /// file). With `--to jsonl`, only this kind is written.
    #[arg(long, value_enum)]
    entity: Option<Entity>,
    /// Output file; stdout if omitted or `-`.
    #[arg(long, short)]
    output: Option<PathBuf>,
    /// Indent JSON.
    #[arg(long)]
    pretty: bool,
    /// Keep null values and empty lists in JSON.
    #[arg(long)]
    keep_empty: bool,
    /// CSV field delimiter.
    #[arg(long, default_value_t = ',')]
    delimiter: char,
    #[command(flatten)]
    input: InputArgs,
}

fn event_kind(e: &Event) -> Option<(&'static str, Option<Entity>)> {
    Some(match e {
        Event::Start(_) => ("header", None),
        Event::ClassifierEnd(_) => ("classifier", None),
        Event::Group(_) => ("group", None),
        Event::Property(_) => ("property", None),
        Event::PriceType(_) => ("price_type", None),
        Event::Warehouse(_) => ("warehouse", None),
        Event::Unit(_) => ("unit", None),
        Event::Category(_) => ("category", None),
        Event::CatalogEnd(_) => ("catalog", None),
        Event::Product(_) => ("product", Some(Entity::Products)),
        Event::OfferPackageEnd(_) => ("offer_package", None),
        Event::Offer(_) => ("offer", Some(Entity::Offers)),
        Event::OfferPackageChangesEnd(_) => ("offer_package_changes", None),
        Event::OfferChange(_) => ("offer_change", Some(Entity::Offers)),
        Event::Document(_) => ("document", Some(Entity::Documents)),
        Event::Counterparty(_) => ("counterparty", None),
        Event::Unknown(_) => ("unknown", None),
        _ => return None,
    })
}

fn data_of(e: &Event, keep_empty: bool) -> serde_json::Result<serde_json::Value> {
    Ok(match to_json(e, keep_empty)? {
        serde_json::Value::Object(mut m) => m.remove("data").unwrap_or_default(),
        other => other,
    })
}

fn jsonl(args: &ConvertArgs, out: &mut dyn Write) -> anyhow::Result<()> {
    for event in StreamReader::open(&args.file, args.input.options()?)? {
        let event = event?;
        let Some((kind, entity)) = event_kind(&event) else {
            continue;
        };
        if args.entity.is_some() && entity != args.entity {
            continue;
        }
        let line = serde_json::json!({ "type": kind, "data": data_of(&event, args.keep_empty)? });
        serde_json::to_writer(&mut *out, &line)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn join(items: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    items
        .into_iter()
        .map(|s| s.as_ref().to_owned())
        .collect::<Vec<_>>()
        .join("|")
}

fn opt(s: Option<&str>) -> String {
    s.unwrap_or_default().to_owned()
}

fn product_row(p: &Product) -> Vec<String> {
    let unit = p.base_unit.as_ref();
    vec![
        p.id.clone(),
        opt(p.article.as_deref()),
        opt(p.barcode.as_deref()),
        p.name.clone(),
        join(&p.group_ids),
        opt(unit.and_then(|u| u.code.as_deref())),
        opt(unit.and_then(|u| u.full_name.as_deref())),
        opt(p.country.as_deref()),
        opt(p.status.as_ref().map(commerceml::ProductStatus::as_str)),
        p.is_deleted().to_string(),
        join(&p.pictures),
        join(
            p.property_values
                .iter()
                .map(|v| format!("{}={}", v.id, v.values.join(";"))),
        ),
        opt(p.description.as_deref()),
    ]
}

const PRODUCT_HEADER: [&str; 13] = [
    "id",
    "article",
    "barcode",
    "name",
    "groups",
    "unit_code",
    "unit_name",
    "country",
    "status",
    "deleted",
    "pictures",
    "properties",
    "description",
];

fn offer_rows(o: &Offer, price_types: &[(String, String)]) -> Vec<Vec<String>> {
    let base = vec![
        o.id().to_owned(),
        o.product_id().to_owned(),
        opt(o.characteristic_id()),
        o.product.name.clone(),
        opt(o.product.article.as_deref()),
        o.total_quantity().map(|q| q.to_string()).unwrap_or_default(),
    ];
    if o.prices.is_empty() {
        let mut row = base;
        row.extend(std::iter::repeat_n(String::new(), 5));
        return vec![row];
    }
    o.prices
        .iter()
        .map(|p| {
            let type_name = price_types
                .iter()
                .find(|(id, _)| *id == p.price_type_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            let mut row = base.clone();
            row.extend([
                p.price_type_id.clone(),
                type_name,
                p.price_per_unit.map(|d| d.to_string()).unwrap_or_default(),
                opt(p.currency.as_deref()),
                opt(p.unit.unit.as_deref()),
            ]);
            row
        })
        .collect()
}

const OFFER_HEADER: [&str; 11] = [
    "offer_id",
    "product_id",
    "characteristic_id",
    "name",
    "article",
    "quantity",
    "price_type_id",
    "price_type",
    "price",
    "currency",
    "unit",
];

fn document_rows(d: &Document) -> Vec<Vec<String>> {
    let buyer = d
        .counterparty(&Role::Buyer)
        .and_then(|c| c.display_name())
        .unwrap_or_default()
        .to_owned();
    let head = [
        d.id.clone(),
        d.number.clone(),
        d.date.clone(),
        opt(d.operation.as_ref().map(commerceml::BusinessOperation::as_str)),
        buyer,
        d.amount.map(|a| a.to_string()).unwrap_or_default(),
    ];
    d.items
        .iter()
        .map(|i| {
            let mut row = head.to_vec();
            row.extend([
                i.product.id.clone(),
                i.product.name.clone(),
                i.quantity.map(|q| q.to_string()).unwrap_or_default(),
                i.price_per_unit.map(|q| q.to_string()).unwrap_or_default(),
                i.amount.map(|q| q.to_string()).unwrap_or_default(),
            ]);
            row
        })
        .collect()
}

const DOCUMENT_HEADER: [&str; 11] = [
    "document_id",
    "number",
    "date",
    "operation",
    "buyer",
    "document_amount",
    "item_id",
    "item_name",
    "quantity",
    "price",
    "amount",
];

fn csv(args: &ConvertArgs, out: &mut dyn Write) -> anyhow::Result<()> {
    let mut reader = StreamReader::open(&args.file, args.input.options()?)?;
    let mut csv = Csv::new(out, args.delimiter);
    let mut entity = args.entity;
    let mut price_types: Vec<(String, String)> = Vec::new();
    let mut header_written = false;
    for event in reader.by_ref() {
        let event = event?;
        if entity.is_none() {
            entity = match &event {
                Event::CatalogStart(_) => Some(Entity::Products),
                Event::OfferPackageStart(_) => Some(Entity::Offers),
                Event::Document(_) => Some(Entity::Documents),
                _ => None,
            };
        }
        let Some(kind) = entity else { continue };
        if !header_written {
            match kind {
                Entity::Products => csv.row(&PRODUCT_HEADER)?,
                Entity::Offers => csv.row(&OFFER_HEADER)?,
                Entity::Documents => csv.row(&DOCUMENT_HEADER)?,
            }
            header_written = true;
        }
        match (&event, kind) {
            (Event::PriceType(t), _) => price_types.push((t.id.clone(), t.name.clone().unwrap_or_default())),
            (Event::OfferPackageStart(p), _) => price_types.extend(
                p.price_types
                    .iter()
                    .map(|t| (t.id.clone(), t.name.clone().unwrap_or_default())),
            ),
            (Event::Product(p), Entity::Products) => csv.row(&product_row(p))?,
            (Event::Offer(o), Entity::Offers) => {
                for row in offer_rows(o, &price_types) {
                    csv.row(&row)?;
                }
            }
            (Event::Document(d), Entity::Documents) => {
                for row in document_rows(d) {
                    csv.row(&row)?;
                }
            }
            _ => {}
        }
    }
    if !header_written {
        anyhow::bail!("the file has no products, offers or documents to export");
    }
    csv.finish()?;
    for w in reader.take_warnings() {
        eprintln!("warning: {w}");
    }
    Ok(())
}

pub(crate) fn run(args: &ConvertArgs) -> anyhow::Result<()> {
    let mut out = open_output(args.output.as_deref())?;
    match args.to {
        Target::Json => {
            let parsed = commerceml::ParseOptions::default()
                .encoding(args.input.options()?.forced_encoding())
                .parse_path(&args.file)?;
            for w in &parsed.warnings {
                eprintln!("warning: {w}");
            }
            let value = to_json(&parsed.info, args.keep_empty)?;
            if args.pretty {
                serde_json::to_writer_pretty(&mut out, &value)?;
            } else {
                serde_json::to_writer(&mut out, &value)?;
            }
            out.write_all(b"\n")?;
        }
        Target::Jsonl => jsonl(args, &mut out)?,
        Target::Csv => csv(args, &mut out)?,
    }
    out.flush()?;
    Ok(())
}
