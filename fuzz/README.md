# Fuzzing

Requires nightly Rust and [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run parse_slice fuzz/corpus_seed
cargo +nightly fuzz run stream fuzz/corpus_seed
cargo +nightly fuzz run roundtrip fuzz/corpus_seed
```

`corpus_seed/` holds copies of the synthetic test fixtures as a starting corpus.
CI runs every target for 60 seconds.
