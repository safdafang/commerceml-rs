//! `commerceml validate`.

use std::path::PathBuf;

use clap::Args;
use commerceml::stream::{Event, StreamReader};
use commerceml::validate::{Issue, Severity, Validator};
use commerceml::{Error, Position, Warning};
use serde::Serialize;

use crate::{Format, InputArgs};

#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {
    /// Files to check.
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Treat parser warnings (unknown elements…) as errors.
    #[arg(long)]
    deny_warnings: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Print at most this many warnings and issues per file (0 = all).
    #[arg(long, default_value_t = 50)]
    limit: usize,
    #[command(flatten)]
    input: InputArgs,
}

#[derive(Debug, Serialize)]
struct FileReport {
    path: PathBuf,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_position: Option<Position>,
    encoding: Option<String>,
    schema_version: Option<String>,
    warnings: Vec<Warning>,
    suppressed_warnings: u64,
    issues: Vec<Issue>,
}

fn check(path: &PathBuf, args: &ValidateArgs) -> anyhow::Result<FileReport> {
    let options = args.input.options()?;
    let mut report = FileReport {
        path: path.clone(),
        ok: false,
        error: None,
        error_position: None,
        encoding: None,
        schema_version: None,
        warnings: Vec::new(),
        suppressed_warnings: 0,
        issues: Vec::new(),
    };
    let mut reader = match StreamReader::open(path, options.clone()) {
        Ok(r) => r,
        Err(Error::Io(e)) => {
            return Err(anyhow::Error::new(e).context(format!("cannot open {}", path.display())));
        }
        Err(e) => {
            report.error = Some(e.to_string());
            return Ok(report);
        }
    };
    report.encoding = Some(reader.encoding().to_string());
    let mut validator = Validator::new();
    let mut fatal = None;
    for event in reader.by_ref() {
        match event {
            Ok(e) => {
                if let Event::Start(h) = &e {
                    report.schema_version = Some(h.schema_version.clone());
                }
                validator.event(&e);
            }
            Err(e) => {
                fatal = Some(e);
                break;
            }
        }
    }
    report.warnings = reader.take_warnings();
    report.suppressed_warnings = reader.suppressed_warnings();
    report.issues = validator.finish();

    let mut positions: Vec<&mut Position> = report.warnings.iter_mut().map(|w| &mut w.position).collect();
    let mut error_position = fatal.as_ref().and_then(Error::position);
    if let Some(p) = error_position.as_mut() {
        positions.push(p);
    }
    commerceml::error::resolve_positions_in_file(path, options.forced_encoding(), &mut positions);
    if let Some(e) = fatal {
        report.error = Some(match (&e, error_position) {
            (Error::Xml { message, .. }, Some(p)) => format!("malformed XML at {p}: {message}"),
            (Error::NotCommerceMl { found, .. }, Some(p)) => {
                format!("not a CommerceML document: root element is <{found}> at {p}")
            }
            _ => e.to_string(),
        });
        report.error_position = error_position;
    }
    let has_errors = report.issues.iter().any(|i| i.severity == Severity::Error);
    let warnings_fail = args.deny_warnings && !report.warnings.is_empty();
    report.ok = report.error.is_none() && !has_errors && !warnings_fail;
    Ok(report)
}

fn print_text(r: &FileReport, limit: usize) {
    let limit = if limit == 0 { usize::MAX } else { limit };
    let status = if r.ok { "ok" } else { "FAILED" };
    let errors = r.issues.iter().filter(|i| i.severity == Severity::Error).count();
    println!(
        "{}: {status} — {} error(s), {} warning(s), {} parser warning(s){}",
        r.path.display(),
        errors + usize::from(r.error.is_some()),
        r.issues.len() - errors,
        r.warnings.len() as u64 + r.suppressed_warnings,
        match (&r.schema_version, &r.encoding) {
            (Some(v), Some(e)) => format!(" [ВерсияСхемы {v}, {e}]"),
            _ => String::new(),
        }
    );
    if let Some(e) = &r.error {
        println!("  error: {e}");
    }
    for i in r.issues.iter().take(limit) {
        println!("  {i}");
    }
    for w in r.warnings.iter().take(limit) {
        println!("  parser warning: {w}");
    }
    let hidden = r.issues.len().saturating_sub(limit) + r.warnings.len().saturating_sub(limit);
    if hidden > 0 || r.suppressed_warnings > 0 {
        println!(
            "  … {} more (use --limit 0 to see all)",
            hidden as u64 + r.suppressed_warnings
        );
    }
}

pub(crate) fn run(args: &ValidateArgs) -> anyhow::Result<bool> {
    let mut reports = Vec::new();
    for path in &args.files {
        reports.push(check(path, args)?);
    }
    match args.format {
        Format::Text => reports.iter().for_each(|r| print_text(r, args.limit)),
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "files": reports }))?
        ),
    }
    Ok(reports.iter().all(|r| r.ok))
}
