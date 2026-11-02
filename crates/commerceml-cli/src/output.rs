//! Output helpers: JSON pruning, CSV, sizes.

use std::io::{self, Write};

use serde_json::Value;

/// Removes `null`, empty arrays and empty objects, recursively, so JSON
/// output shows only what the file contains.
pub(crate) fn prune(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                prune(v);
            }
            map.retain(|_, v| !is_empty(v));
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                prune(v);
            }
        }
        _ => {}
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// Serializes `value` to JSON, pruned unless `keep_empty`.
pub(crate) fn to_json<T: serde::Serialize>(value: &T, keep_empty: bool) -> serde_json::Result<Value> {
    let mut v = serde_json::to_value(value)?;
    if !keep_empty {
        prune(&mut v);
    }
    Ok(v)
}

/// A minimal RFC 4180 CSV writer.
pub(crate) struct Csv<W: Write> {
    out: W,
    delimiter: char,
}

impl<W: Write> Csv<W> {
    pub(crate) fn new(out: W, delimiter: char) -> Self {
        Self { out, delimiter }
    }

    pub(crate) fn row<S: AsRef<str>>(&mut self, fields: &[S]) -> io::Result<()> {
        let mut line = String::new();
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                line.push(self.delimiter);
            }
            let f = f.as_ref();
            if f.contains([self.delimiter, '"', '\n', '\r']) {
                line.push('"');
                line.push_str(&f.replace('"', "\"\""));
                line.push('"');
            } else {
                line.push_str(f);
            }
        }
        line.push_str("\r\n");
        self.out.write_all(line.as_bytes())
    }

    pub(crate) fn finish(mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// `1234567` → `1.2 MB`.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.1} kB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

/// Opens `path` for writing, or stdout for `None` / `-`.
pub(crate) fn open_output(path: Option<&std::path::Path>) -> io::Result<Box<dyn Write>> {
    match path {
        Some(p) if p != std::path::Path::new("-") => {
            Ok(Box::new(io::BufWriter::new(std::fs::File::create(p)?)))
        }
        _ => Ok(Box::new(io::BufWriter::new(io::stdout().lock()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prunes_empty_values() {
        let mut v =
            serde_json::json!({"a": null, "b": [], "c": {"d": null}, "e": "", "f": [{"g": null}], "h": 0});
        prune(&mut v);
        assert_eq!(v, serde_json::json!({"e": "", "f": [{}], "h": 0}));
    }

    #[test]
    fn csv_quoting() {
        let mut buf = Vec::new();
        let mut csv = Csv::new(&mut buf, ',');
        csv.row(&["a", "b,c", "d\"e", "f\ng"]).unwrap();
        csv.finish().unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "a,\"b,c\",\"d\"\"e\",\"f\ng\"\r\n"
        );
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1_500), "1.5 kB");
        assert_eq!(human_size(112_000_000), "112.0 MB");
        assert_eq!(human_size(2_000_000_000), "2.0 GB");
    }
}
