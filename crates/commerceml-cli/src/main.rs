//! `commerceml` — command-line tool for CommerceML 2 exports.
//!
//! ```text
//! commerceml validate import.xml offers.xml
//! commerceml stats import.xml
//! commerceml convert offers.xml --to csv --entity offers -o prices.csv
//! commerceml diff old/import.xml new/import.xml
//! commerceml generate --products 100000 --out synthetic/
//! ```

#![forbid(unsafe_code)]

mod convert;
mod diff;
mod output;
mod stats;
mod validate;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use commerceml::{Encoding, ParseOptions};

/// Validate, inspect, convert and diff CommerceML 2 files (1C ↔ website exchange).
#[derive(Debug, Parser)]
#[command(name = "commerceml", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check files: well-formedness, unknown elements, required fields, references.
    Validate(validate::ValidateArgs),
    /// Print a summary: counts of groups, products, offers, prices, documents.
    Stats(stats::StatsArgs),
    /// Convert to JSON, JSON Lines or CSV.
    Convert(convert::ConvertArgs),
    /// Compare two exports: added, removed and changed entities.
    Diff(diff::DiffArgs),
    /// Generate synthetic import.xml, offers.xml and orders.xml.
    Generate(GenerateArgs),
}

/// Options shared by commands that read files.
#[derive(Debug, Clone, Args)]
pub(crate) struct InputArgs {
    /// Force the input encoding (e.g. `windows-1251`) instead of detecting it.
    #[arg(long, value_name = "LABEL")]
    encoding: Option<String>,
}

impl InputArgs {
    pub(crate) fn options(&self) -> anyhow::Result<ParseOptions> {
        let encoding = match &self.encoding {
            Some(label) => Some(
                Encoding::for_label(label).ok_or_else(|| anyhow::anyhow!("unknown encoding {label:?}"))?,
            ),
            None => None,
        };
        Ok(ParseOptions::default().encoding(encoding))
    }
}

/// Text for people or JSON for programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Format {
    /// Human-readable text.
    Text,
    /// Machine-readable JSON.
    Json,
}

#[derive(Debug, Args)]
struct GenerateArgs {
    /// Number of products.
    #[arg(long, default_value_t = 1000)]
    products: usize,
    /// Number of orders.
    #[arg(long, default_value_t = 100)]
    orders: usize,
    /// Output directory.
    #[arg(long, short, default_value = "synthetic")]
    out: PathBuf,
    /// Output encoding.
    #[arg(long, default_value = "utf-8", value_name = "LABEL")]
    encoding: String,
    /// Seed of the generator.
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn generate(args: &GenerateArgs) -> anyhow::Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let encoding = Encoding::for_label(&args.encoding)
        .ok_or_else(|| anyhow::anyhow!("unknown encoding {:?}", args.encoding))?;
    let options = commerceml::WriteOptions::default().encoding(encoding);
    let cfg = commerceml::synth::Config {
        products: args.products,
        seed: args.seed,
        ..commerceml::synth::Config::default()
    };
    std::fs::create_dir_all(&args.out)?;
    let create = |name: &str| -> anyhow::Result<BufWriter<File>> {
        Ok(BufWriter::new(File::create(args.out.join(name))?))
    };
    cfg.write_import(create("import.xml")?, &options)?;
    cfg.write_offers(create("offers.xml")?, &options)?;
    cfg.write_orders(args.orders, create("orders.xml")?, &options)?;
    for name in ["import.xml", "offers.xml", "orders.xml"] {
        let path = args.out.join(name);
        println!(
            "{} ({})",
            path.display(),
            output::human_size(std::fs::metadata(&path)?.len())
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Validate(a) => validate::run(a),
        Command::Stats(a) => stats::run(a).map(|()| true),
        Command::Convert(a) => convert::run(a).map(|()| true),
        Command::Diff(a) => diff::run(a),
        Command::Generate(a) => generate(a).map(|()| true),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
