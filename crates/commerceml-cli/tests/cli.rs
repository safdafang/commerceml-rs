//! End-to-end tests of the `commerceml` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../commerceml/tests/fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_commerceml"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("commerceml-cli-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn validate_reports_and_sets_exit_code() {
    let ok = run(&["validate", path(&fixture("import_2_10.xml"))]);
    assert_eq!(ok.status.code(), Some(0), "{}", stdout(&ok));
    assert!(stdout(&ok).contains("unknown element <Бренд>"));
    assert!(stdout(&ok).contains("line 162, column 5"));

    let strict = run(&["validate", "--deny-warnings", path(&fixture("import_2_10.xml"))]);
    assert_eq!(strict.status.code(), Some(1));

    let bad = run(&["validate", "--format", "json", path(&fixture("offers_2_07.xml"))]);
    assert_eq!(bad.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&bad.stdout).unwrap();
    let file = &json["files"][0];
    assert_eq!(file["ok"], false);
    assert_eq!(file["issues"][1]["code"], "MissingField");
    assert_eq!(file["warnings"][0]["position"]["line"], 97);
}

#[test]
fn validate_malformed_file() {
    let dir = temp_dir("malformed");
    let file = dir.join("broken.xml");
    std::fs::write(
        &file,
        "<КоммерческаяИнформация ВерсияСхемы=\"2.10\">\n<Каталог>\n</Товары>",
    )
    .unwrap();
    let out = run(&["validate", path(&file)]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("error: malformed XML at line 3"),
        "{}",
        stdout(&out)
    );

    let missing = run(&["validate", "does-not-exist.xml"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("cannot open"));
}

#[test]
fn stats_counts_entities() {
    let out = run(&["stats", "--format", "json", path(&fixture("import_2_10.xml"))]);
    assert!(out.status.success());
    let s: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(s["groups"], 4);
    assert_eq!(s["products"], 3);
    assert_eq!(s["products_deleted"], 1);
    assert_eq!(s["unknown_elements"]["Бренд"], 1);
    let text = run(&["stats", path(&fixture("orders_2_03.xml"))]);
    assert!(stdout(&text).contains("documents"));
}

#[test]
fn convert_to_csv_jsonl_and_json() {
    let csv = run(&[
        "convert",
        path(&fixture("offers_2_07.xml")),
        "--to",
        "csv",
        "--delimiter",
        ";",
    ]);
    assert!(csv.status.success());
    let text = stdout(&csv);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("offer_id;product_id;characteristic_id"));
    assert_eq!(lines.len(), 5, "{text}");
    assert!(lines[1].contains(";Розничная;1499.00;"));

    let products = run(&["convert", path(&fixture("import_2_10.xml")), "--to", "csv"]);
    assert!(
        stdout(&products).contains("\"Лёгкий стул.\nВторая строка"),
        "multi-line fields are quoted"
    );

    let docs = run(&[
        "convert",
        path(&fixture("orders_2_03.xml")),
        "--to",
        "csv",
        "--entity",
        "documents",
    ]);
    assert_eq!(stdout(&docs).lines().count(), 3);

    let jsonl = run(&[
        "convert",
        path(&fixture("import_2_10.xml")),
        "--to",
        "jsonl",
        "--entity",
        "products",
    ]);
    let lines: Vec<serde_json::Value> = stdout(&jsonl)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["type"], "product");
    assert_eq!(lines[0]["data"]["id"], "p-chair");
    assert!(
        stdout(&jsonl).starts_with("{\"type\":\"product\",\"data\":{\"id\":"),
        "field order is kept"
    );

    let dir = temp_dir("convert");
    let out = dir.join("out.json");
    let json = run(&[
        "convert",
        path(&fixture("rests_2_09.xml")),
        "--to",
        "json",
        "--pretty",
        "-o",
        path(&out),
    ]);
    assert!(json.status.success());
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        v["offer_package"]["offers"][0]["rests"][0]["warehouse"]["warehouse_id"],
        "wh-main"
    );
    assert!(
        v["offer_package"].get("signatories").is_none(),
        "empty values are pruned"
    );
}

#[test]
fn diff_two_exports() {
    let dir = temp_dir("diff");
    let old = fixture("offers_2_07.xml");
    let new = dir.join("offers.xml");
    let changed = std::fs::read_to_string(&old)
        .unwrap()
        .replace(
            "<ЦенаЗаЕдиницу>1499.00</ЦенаЗаЕдиницу>",
            "<ЦенаЗаЕдиницу>1599.00</ЦенаЗаЕдиницу>",
        )
        .replace("<Ид>p-lamp#</Ид>", "<Ид>p-desk</Ид>");
    std::fs::write(&new, changed).unwrap();

    let same = run(&["diff", path(&old), path(&old)]);
    assert_eq!(same.status.code(), Some(0));
    assert!(stdout(&same).contains("0 added, 0 removed, 0 changed"));

    let out = run(&["diff", "--values", path(&old), path(&new)]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("+ offer p-desk"), "{text}");
    assert!(text.contains("- offer p-lamp#"), "{text}");
    assert!(text.contains("~ offer p-chair «Стул складной»: prices"), "{text}");
    assert!(
        text.contains("prices[0].price_per_unit: \"1499.00\" → \"1599.00\""),
        "{text}"
    );

    let json = run(&["diff", "--format", "json", path(&old), path(&new)]);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(v["changed"][0]["fields"][0]["field"], "prices");
    assert_eq!(v["unchanged"], 1);
}

#[test]
fn generate_then_validate() {
    let dir = temp_dir("generate");
    let out = run(&[
        "generate",
        "--products",
        "200",
        "--orders",
        "5",
        "--out",
        path(&dir),
        "--encoding",
        "windows-1251",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let files: Vec<PathBuf> = ["import.xml", "offers.xml", "orders.xml"]
        .iter()
        .map(|f| dir.join(f))
        .collect();
    let args: Vec<&str> = std::iter::once("validate")
        .chain(files.iter().map(|p| path(p)))
        .collect();
    let check = run(&args);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(check.status.success(), "{}", stdout(&check));
    assert!(stdout(&check).contains("windows-1251"));
}

#[test]
fn unknown_encoding_is_rejected() {
    let out = run(&["stats", "--encoding", "x-nope", path(&fixture("rests_2_09.xml"))]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown encoding"));
}
