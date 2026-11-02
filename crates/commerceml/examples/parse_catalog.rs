//! Loads an `import.xml` into memory and prints the group tree and products.
//!
//! ```sh
//! cargo run -p commerceml --example parse_catalog -- path/to/import.xml
//! ```
//!
//! Without an argument it reads a synthetic fixture from the repository.

use commerceml::{Group, Product};

fn print_groups(groups: &[Group], depth: usize) {
    for g in groups {
        println!("{}{} ({})", "  ".repeat(depth), g.name, g.id);
        print_groups(&g.groups, depth + 1);
    }
}

fn describe(p: &Product) -> String {
    let unit = p
        .base_unit
        .as_ref()
        .and_then(|u| u.full_name.as_deref())
        .unwrap_or("-");
    let article = p.article.as_deref().filter(|a| !a.is_empty()).unwrap_or("-");
    format!(
        "{} | article {article} | unit {unit} | {} picture(s)",
        p.name.trim(),
        p.pictures.len()
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import_2_10.xml").to_owned());
    let parsed = commerceml::from_path(&path)?;
    for w in &parsed.warnings {
        eprintln!("warning: {w}");
    }

    let info = &parsed.info;
    println!(
        "CommerceML {} generated {} ({})",
        info.schema_version, info.generated_at, parsed.encoding
    );
    if let Some(classifier) = &info.classifier {
        println!("\nGroups of «{}»:", classifier.name);
        print_groups(&classifier.groups, 1);
        println!("\nProperties:");
        for p in &classifier.properties {
            println!(
                "  {} ({:?}, {} dictionary values)",
                p.name,
                p.value_type,
                p.dictionary.len()
            );
        }
    }
    println!("\nProducts:");
    for p in info.products() {
        let mark = if p.is_deleted() { " [deleted]" } else { "" };
        println!("  {}{mark}", describe(p));
        for v in &p.property_values {
            let name_and_value = info
                .classifier
                .as_ref()
                .and_then(|c| c.property(&v.id))
                .map(|prop| {
                    let value = v.values.first().map_or("", String::as_str);
                    (prop.name.as_str(), prop.dictionary_value(value).unwrap_or(value))
                });
            if let Some((name, value)) = name_and_value {
                println!("      {name}: {value}");
            }
        }
    }
    Ok(())
}
