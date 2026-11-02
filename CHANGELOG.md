# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

First release (alpha).

### Added

- `commerceml`: data model for CommerceML 2 — classifier, catalog, offer packages
  and their changes, documents, counterparties — with the Russian element names
  in the documentation; exact `Decimal` amounts.
- `commerceml`: streaming reader with constant memory use; whole-file parsing is
  built on it.
- `commerceml`: versions 2.03–2.10 and the de facto extensions of real exports
  (`Остатки`, `ПометкаУдаления`, `ЕдиницыИзмерения`, `Контрагенты`, `Контейнер` …);
  unknown elements become warnings with line and column and are kept as raw XML.
- `commerceml`: UTF-8 and windows-1251 input and output.
- `commerceml`: writer for whole documents and a streaming `write::Writer`;
  lossless `parse → write → parse`.
- `commerceml`: semantic validation, synthetic data generator (feature `synth`),
  optional `serde` support.
- `commerceml-exchange`: server side of the 1C exchange protocol (`checkauth`,
  `init`, `file` in parts, `import` with `progress`, `deactivate`, `complete`,
  `query`, `success`), `FsStorage` and `MemoryStorage`, upload and unzip limits,
  axum router (feature `axum`).
- `commerceml-cli`: `validate`, `stats`, `convert` (JSON, JSON Lines, CSV),
  `diff`, `generate`.

Built against: CommerceML 2.10 XSD and logical description (1C, 2018), the
"Протокол обмена с сайтом" page on v8.1c.ru.
