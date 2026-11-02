//! Throughput on a synthetic `import.xml` of about 112 MB and its `offers.xml`.
//!
//! The files are generated once into `target/bench-data`. Run with
//! `cargo bench -p commerceml --features synth`.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use commerceml::stream::{Event, StreamReader};
use commerceml::synth::Config;
use commerceml::{Encoding, ParseOptions, WriteOptions};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const PRODUCTS: usize = 40_000;

fn data_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    );
    let dir = target.join("bench-data");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Generates `name` with `write` unless it already exists.
fn ensure(name: &str, write: impl FnOnce(BufWriter<File>) -> BufWriter<File>) -> PathBuf {
    let path = data_dir().join(name);
    if !path.exists() {
        let tmp = path.with_extension("tmp");
        let mut out = write(BufWriter::new(File::create(&tmp).unwrap()));
        out.flush().unwrap();
        drop(out);
        std::fs::rename(&tmp, &path).unwrap();
    }
    path
}

fn config() -> Config {
    Config {
        products: PRODUCTS,
        ..Config::default()
    }
}

fn import_utf8() -> PathBuf {
    ensure(&format!("import-{PRODUCTS}.xml"), |w| {
        config().write_import(w, &WriteOptions::default()).unwrap()
    })
}

fn import_1251() -> PathBuf {
    ensure(&format!("import-{PRODUCTS}-1251.xml"), |w| {
        let options = WriteOptions::default().encoding(Encoding::WINDOWS_1251);
        config().write_import(w, &options).unwrap()
    })
}

fn offers() -> PathBuf {
    ensure(&format!("offers-{PRODUCTS}.xml"), |w| {
        config().write_offers(w, &WriteOptions::default()).unwrap()
    })
}

fn count_events(path: &Path) -> usize {
    StreamReader::open(path, ParseOptions::default())
        .unwrap()
        .filter(|e| matches!(e, Ok(Event::Product(_) | Event::Offer(_))))
        .count()
}

fn benches(c: &mut Criterion) {
    let import = import_utf8();
    let import_cp = import_1251();
    let offers = offers();
    let size = |p: &Path| std::fs::metadata(p).unwrap().len();
    eprintln!(
        "import.xml: {} MB, import (windows-1251): {} MB, offers.xml: {} MB",
        size(&import) / 1_000_000,
        size(&import_cp) / 1_000_000,
        size(&offers) / 1_000_000
    );

    let mut g = c.benchmark_group("parse");
    g.sample_size(10).measurement_time(Duration::from_secs(40));

    g.throughput(Throughput::Bytes(size(&import)));
    g.bench_function("stream import.xml (UTF-8)", |b| {
        b.iter(|| assert_eq!(count_events(&import), PRODUCTS));
    });
    g.bench_function("load import.xml into memory", |b| {
        b.iter(|| {
            assert_eq!(
                commerceml::from_path(&import).unwrap().info.products().len(),
                PRODUCTS
            );
        });
    });

    g.throughput(Throughput::Bytes(size(&import_cp)));
    g.bench_function("stream import.xml (windows-1251)", |b| {
        b.iter(|| assert_eq!(count_events(&import_cp), PRODUCTS));
    });

    g.throughput(Throughput::Bytes(size(&offers)));
    g.bench_function("stream offers.xml", |b| {
        b.iter(|| assert!(count_events(&offers) >= PRODUCTS));
    });
    g.finish();

    let info = commerceml::from_path(&import).unwrap().info;
    let mut g = c.benchmark_group("write");
    g.sample_size(10).measurement_time(Duration::from_secs(30));
    g.throughput(Throughput::Bytes(size(&import)));
    g.bench_function("write import.xml (UTF-8)", |b| {
        b.iter(|| WriteOptions::default().write(&info, std::io::sink()).unwrap());
    });
    g.finish();
}

criterion_group!(parse, benches);
criterion_main!(parse);
