# commerceml

Parser and writer for **CommerceML 2**, the XML format 1C:Enterprise uses to
exchange catalogs, prices, stock and orders with websites.

- whole-file parsing (`commerceml::from_path`) or streaming with constant
  memory (`commerceml::stream::StreamReader`);
- versions 2.03–2.10, UTF-8 and windows-1251, namespaced or not;
- de facto extensions of real 1C exports are modelled; unknown elements are
  kept as raw XML and reported as warnings with line and column;
- writing whole documents or section by section; parse → write → parse is
  lossless;
- semantic validation, optional `serde`, a synthetic data generator
  (feature `synth`).

```rust,no_run
fn main() -> Result<(), commerceml::Error> {
    let parsed = commerceml::from_path("import.xml")?;
    for product in parsed.info.products() {
        println!("{} {}", product.id, product.name);
    }
    Ok(())
}
```

See the [repository](https://github.com/safdafang/commerceml-rs) for the
exchange server, the CLI, benchmarks and notes on the standard.

License: MIT OR Apache-2.0.
