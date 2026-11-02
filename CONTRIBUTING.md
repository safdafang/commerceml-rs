# Contributing

Thanks for helping! Bug reports with a sample export (anonymised) are the most
useful contribution: every 1C configuration writes CommerceML a little
differently.

## Ground rules

- **Nothing invented.** Every element, attribute and protocol step must come from
  a source: the 1C XSD and logical description, the protocol page on v8.1c.ru,
  the 1C-Bitrix documentation or a real export. If the source is unclear, write
  `TODO(spec)` in [docs/spec-notes.md](docs/spec-notes.md) and explain it in the
  pull request.
- **Synthetic data only.** No real companies, people, INN/KPP or prices in
  fixtures, tests or issues. Use `commerceml::synth` or made-up names.
- Amounts and quantities are `Decimal` — never `f64`.
- No `unsafe` in the library crates; every public item is documented.
- Code, comments and documentation are in English.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `test:`, `chore:` …), scopes `core`, `exchange`, `cli`.

## Reporting a problem with an export

1. Replace names, INN/KPP, addresses, phone numbers and prices with made-up
   values; keep the element structure.
2. Attach the smallest fragment that shows the problem and the output of
   `commerceml validate --format json`.
3. Say which configuration and version produced it, if you know.

## Development setup

```console
rustup toolchain install stable 1.88
cargo build --workspace --all-features
```

## Checks

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo +1.88 test --workspace --all-features --locked
cargo llvm-cov -p commerceml --all-features --fail-under-lines 80
```

Golden files, property tests, benchmarks and fuzzing:

```console
UPDATE_GOLDEN=1 cargo test -p commerceml --features serde --test golden
PROPTEST_CASES=5000 cargo test --release -p commerceml --test properties
cargo bench -p commerceml --features synth       # generates a ~110 MB file once
cargo +nightly fuzz run parse_slice fuzz/corpus_seed
```

## Supporting a new element

1. Add the field to the model (`crates/commerceml/src/model`), the reader
   (`read.rs`) and the writer (`write.rs`).
2. Add it to a fixture in `crates/commerceml/tests/fixtures`, regenerate the
   golden files and review the diff.
3. Extend the strategies in `tests/properties.rs`, so round trips cover it.
4. Record where the element comes from in `docs/spec-notes.md`.
5. Add a `CHANGELOG.md` entry.

## Releasing

Publishing uses crates.io [Trusted Publishing](https://crates.io/docs/trusted-publishing)
from `.github/workflows/release.yml`; no API token is stored.

One-time setup (maintainer):

1. On crates.io, add a trusted publisher for `commerceml`, `commerceml-exchange`
   and `commerceml-cli`: the GitHub owner and repository, workflow `release.yml`,
   environment `crates-io`.
2. In the GitHub repository, create the environment `crates-io` with required
   reviewers.

Each release:

1. Update `version` in the workspace `Cargo.toml` and `CHANGELOG.md`.
2. Tag `vX.Y.Z` and publish a GitHub release. The workflow builds the CLI
   binaries, attaches them to the release and waits for approval of the
   `crates-io` environment before publishing.
