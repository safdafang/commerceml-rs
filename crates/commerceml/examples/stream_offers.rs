//! Streams an `offers.xml` of any size and prints a price list as CSV,
//! keeping memory use constant.
//!
//! ```sh
//! cargo run --release -p commerceml --example stream_offers -- offers.xml > prices.csv
//! ```

use commerceml::ParseOptions;
use commerceml::stream::{Event, StreamReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/offers_2_07.xml").to_owned());
    let mut reader = StreamReader::open(&path, ParseOptions::default())?;
    let mut price_types = Vec::new();
    println!("offer_id;name;price_type;price;currency;stock");
    for event in reader.by_ref() {
        match event? {
            Event::OfferPackageStart(package) => {
                price_types = package
                    .price_types
                    .into_iter()
                    .map(|t| (t.id, t.name.unwrap_or_default()))
                    .collect();
            }
            Event::Offer(offer) => {
                let stock = offer.total_quantity().map(|q| q.to_string()).unwrap_or_default();
                for price in &offer.prices {
                    let type_name = price_types
                        .iter()
                        .find(|(id, _)| *id == price.price_type_id)
                        .map_or(price.price_type_id.as_str(), |(_, name)| name.as_str());
                    println!(
                        "{};{};{};{};{};{}",
                        offer.id(),
                        offer.product.name.trim(),
                        type_name,
                        price.price_per_unit.map(|p| p.to_string()).unwrap_or_default(),
                        price.currency.as_deref().unwrap_or(""),
                        stock
                    );
                }
            }
            _ => {}
        }
    }
    for w in reader.take_warnings() {
        eprintln!("warning: {w}");
    }
    Ok(())
}
