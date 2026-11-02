//! `commerceml diff`.
//!
//! The old file is reduced to one fingerprint per field of every entity; the
//! new file is streamed and compared against it. Memory is proportional to
//! the number of entities, not to the file size. With `--values` the old
//! entities are kept whole to show old and new values.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use clap::Args;
use commerceml::Group;
use commerceml::stream::{Event, StreamReader};
use serde::Serialize;
use serde_json::Value;

use crate::output::to_json;
use crate::{Format, InputArgs};

#[derive(Debug, Args)]
pub(crate) struct DiffArgs {
    /// The earlier export.
    old: PathBuf,
    /// The later export.
    new: PathBuf,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Show old and new values of changed fields (keeps the old file's
    /// entities in memory).
    #[arg(long)]
    values: bool,
    #[command(flatten)]
    input: InputArgs,
}

/// Fields of one entity: fingerprints, and the values with `--values`.
#[derive(Debug)]
struct Entry {
    name: String,
    fields: BTreeMap<String, u64>,
    values: Option<serde_json::Map<String, Value>>,
}

fn fingerprint(v: &Value) -> u64 {
    let mut h = DefaultHasher::new();
    v.to_string().hash(&mut h);
    h.finish()
}

fn entry(value: &Value, name: String, keep: bool) -> Entry {
    let map = value.as_object().cloned().unwrap_or_default();
    Entry {
        name,
        fields: map.iter().map(|(k, v)| (k.clone(), fingerprint(v))).collect(),
        values: keep.then_some(map),
    }
}

/// Entities keyed by kind, then by `Ид`.
type Index = BTreeMap<&'static str, HashMap<String, Entry>>;

fn flatten_groups(g: &Group, out: &mut Vec<Group>) {
    let mut own = g.clone();
    own.groups.clear();
    out.push(own);
    for c in &g.groups {
        flatten_groups(c, out);
    }
}

/// Calls `f(kind, id, name, json)` for every comparable entity in `path`.
fn walk(
    path: &PathBuf,
    input: &InputArgs,
    mut f: impl FnMut(&'static str, String, String, Value),
) -> anyhow::Result<()> {
    for event in StreamReader::open(path, input.options()?)? {
        match event? {
            Event::Group(g) => {
                let mut all = Vec::new();
                flatten_groups(&g, &mut all);
                for g in all {
                    f("group", g.id.clone(), g.name.clone(), to_json(&g, false)?);
                }
            }
            Event::Property(p) => f("property", p.id.clone(), p.name.clone(), to_json(&p, false)?),
            Event::PriceType(t) => {
                let name = t.name.clone().unwrap_or_default();
                f("price_type", t.id.clone(), name, to_json(&t, false)?);
            }
            Event::Warehouse(w) => {
                let name = w.name.clone().unwrap_or_default();
                f("warehouse", w.id.clone(), name, to_json(&w, false)?);
            }
            Event::Product(p) => f("product", p.id.clone(), p.name.clone(), to_json(&p, false)?),
            Event::Offer(o) => {
                let mut v = to_json(&o, false)?;
                if let Some(obj) = v.as_object_mut()
                    && let Some(Value::Object(product)) = obj.remove("product")
                {
                    obj.extend(product);
                }
                f("offer", o.id().to_owned(), o.product.name.clone(), v);
            }
            Event::Document(d) => {
                let name = format!("№{} от {}", d.number, d.date);
                f("document", d.id.clone(), name, to_json(&d, false)?);
            }
            Event::Counterparty(c) => {
                let name = c.display_name().unwrap_or_default().to_owned();
                f(
                    "counterparty",
                    c.id.clone().unwrap_or_default(),
                    name,
                    to_json(&c, false)?,
                );
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct Change {
    kind: &'static str,
    id: String,
    name: String,
    fields: Vec<FieldChange>,
}

#[derive(Debug, Serialize)]
struct FieldChange {
    field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Ref {
    kind: &'static str,
    id: String,
    name: String,
}

#[derive(Debug, Default, Serialize)]
struct Report {
    added: Vec<Ref>,
    removed: Vec<Ref>,
    changed: Vec<Change>,
    unchanged: usize,
}

pub(crate) fn run(args: &DiffArgs) -> anyhow::Result<bool> {
    let mut old: Index = BTreeMap::new();
    walk(&args.old, &args.input, |kind, id, name, v| {
        old.entry(kind)
            .or_default()
            .insert(id, entry(&v, name, args.values));
    })?;

    let mut report = Report::default();
    walk(&args.new, &args.input, |kind, id, name, v| {
        let new = entry(&v, name, true);
        match old.get_mut(kind).and_then(|m| m.remove(&id)) {
            None => report.added.push(Ref {
                kind,
                id,
                name: new.name,
            }),
            Some(before) if before.fields == new.fields => report.unchanged += 1,
            Some(before) => {
                let keys: BTreeSet<&String> = before.fields.keys().chain(new.fields.keys()).collect();
                let fields = keys
                    .into_iter()
                    .filter(|k| before.fields.get(*k) != new.fields.get(*k))
                    .map(|k| FieldChange {
                        field: k.clone(),
                        old: before.values.as_ref().and_then(|m| m.get(k).cloned()),
                        new: args
                            .values
                            .then(|| new.values.as_ref().and_then(|m| m.get(k).cloned()))
                            .flatten(),
                    })
                    .collect();
                report.changed.push(Change {
                    kind,
                    id,
                    name: new.name,
                    fields,
                });
            }
        }
    })?;
    for (kind, rest) in old {
        let mut rest: Vec<_> = rest.into_iter().collect();
        rest.sort_by(|a, b| a.0.cmp(&b.0));
        report.removed.extend(rest.into_iter().map(|(id, e)| Ref {
            kind,
            id,
            name: e.name,
        }));
    }

    let identical = report.added.is_empty() && report.removed.is_empty() && report.changed.is_empty();
    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Text => print_text(&report),
    }
    Ok(identical)
}

fn short(v: Option<&Value>) -> String {
    let s = v.map_or_else(|| "—".to_owned(), ToString::to_string);
    if s.chars().count() > 80 {
        format!("{}…", s.chars().take(79).collect::<String>())
    } else {
        s
    }
}

/// Collects `path: old → new` for every differing leaf of two JSON values.
fn leaf_changes(path: &str, old: Option<&Value>, new: Option<&Value>, out: &mut Vec<String>) {
    match (old, new) {
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for k in keys {
                leaf_changes(&format!("{path}.{k}"), a.get(k), b.get(k), out);
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            for i in 0..a.len().max(b.len()) {
                leaf_changes(&format!("{path}[{i}]"), a.get(i), b.get(i), out);
            }
        }
        (a, b) if a != b => out.push(format!("{path}: {} → {}", short(a), short(b))),
        _ => {}
    }
}

fn print_text(r: &Report) {
    for a in &r.added {
        println!("+ {} {} «{}»", a.kind, a.id, a.name.trim());
    }
    for d in &r.removed {
        println!("- {} {} «{}»", d.kind, d.id, d.name.trim());
    }
    for c in &r.changed {
        let names: Vec<&str> = c.fields.iter().map(|f| f.field.as_str()).collect();
        println!("~ {} {} «{}»: {}", c.kind, c.id, c.name.trim(), names.join(", "));
        for f in c.fields.iter().filter(|f| f.old.is_some() || f.new.is_some()) {
            let mut lines = Vec::new();
            leaf_changes(&f.field, f.old.as_ref(), f.new.as_ref(), &mut lines);
            for line in lines {
                println!("    {line}");
            }
        }
    }
    println!(
        "{} added, {} removed, {} changed, {} unchanged",
        r.added.len(),
        r.removed.len(),
        r.changed.len(),
        r.unchanged
    );
}
