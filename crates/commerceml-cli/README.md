# commerceml-cli

Command-line tool for CommerceML 2 exports from 1C:Enterprise.

```sh
cargo install commerceml-cli

commerceml validate import.xml offers.xml      # exit code 1 on errors
commerceml stats import.xml                     # counts and throughput
commerceml convert offers.xml --to csv -o prices.csv
commerceml convert import.xml --to jsonl --entity products
commerceml convert orders.xml --to json --pretty
commerceml diff old/import.xml new/import.xml --values
commerceml generate --products 100000 --out synthetic/
```

Large files are streamed: `validate`, `stats`, `convert --to jsonl/csv` and
the new side of `diff` use constant memory. Run `commerceml help <command>`
for options.

License: MIT OR Apache-2.0.
