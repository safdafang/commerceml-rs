//! Writes a synthetic `import.xml`, `offers.xml` and `orders.xml` into a
//! directory. Needs the `synth` feature.
//!
//! ```sh
//! cargo run --release -p commerceml --features synth --example generate -- out-dir 100000
//! ```

use std::fs::File;
use std::io::BufWriter;

use commerceml::WriteOptions;
use commerceml::synth::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "synthetic".into()));
    let products: usize = args.next().map_or(Ok(1000), |n| n.parse())?;
    std::fs::create_dir_all(&dir)?;

    let cfg = Config {
        products,
        ..Config::default()
    };
    let options = WriteOptions::default();
    cfg.write_import(BufWriter::new(File::create(dir.join("import.xml"))?), &options)?;
    cfg.write_offers(BufWriter::new(File::create(dir.join("offers.xml"))?), &options)?;
    cfg.write_orders(
        products / 10 + 1,
        BufWriter::new(File::create(dir.join("orders.xml"))?),
        &options,
    )?;

    for name in ["import.xml", "offers.xml", "orders.xml"] {
        let size = std::fs::metadata(dir.join(name))?.len();
        println!("{} {} kB", dir.join(name).display(), size / 1000);
    }
    Ok(())
}
