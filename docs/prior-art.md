# Prior-art research

Date of research: **2026-11-02**.

Goal: confirm that no Rust crate already parses or writes CommerceML 2 or
implements the 1C ↔ website exchange protocol before writing one.

A crate counts as an existing solution only if it is a library, written in
Rust, covers a noticeable part of the standard and is maintained.

## Where we looked

| Source | Queries |
|---|---|
| crates.io API (`/api/v1/crates?q=`) | `commerceml`, `commerce ml`, `cml`, `1c`, `1с` (Cyrillic), `1c exchange`, `1c-exchange`, `обмен 1с`, `КоммерческаяИнформация`, `onec` |
| crates.io exact names | `commerceml`, `commerceml2`, `cml2`, `commerceml-rs`, `cml`, `onec`, `1c` |
| docs.rs search | `commerceml` |
| lib.rs search | `commerceml` (timed out twice; lib.rs indexes crates.io, which was already checked) |
| GitHub repositories | `commerceml language:rust`, `commerceml`, `commerceml2`, `1c exchange language:rust`, `cml 1c language:rust`, `1c-exchange`, `1c language:rust`, `1С language:rust` |
| GitHub code search (Rust files) | `КоммерческаяИнформация`, `ПакетПредложений`, `mode=checkauth`, `commerceml` |

## Results

### Rust

Nothing implements CommerceML.

| Item | Kind | What it is | Covers our task? |
|---|---|---|---|
| `commerceml`, `commerceml2`, `cml2`, `commerceml-rs` on crates.io | names | not registered (HTTP 404) | — |
| `cml` crate | crate | Cisco Modeling Labs API client | no |
| `cml-*` crates | crates | Cardano SDK, "Content Markup Language" | no |
| `native_api_1c`, `addin1c`, `v8unpack`, `eska-*`, `bsl-analyzer`… | crates/repos | 1C add-ins, config unpacking, BSL tooling, log parsers | no — other 1C areas |
| `Thomblin/xml_schema_generator` | repo | uses a CommerceML fragment as a test input | no |
| `thevladbog/markiro` | repo (TypeScript, Rust parts) | product with a `commerceml` channel name in license code | no |
| docs.rs search `commerceml` | — | "No results" | — |

### Other languages

| Project | Language | Stars | Last push | Licence | Coverage |
|---|---|---|---|---|---|
| [sgtpep/woocommerce-1c](https://github.com/sgtpep/woocommerce-1c) | PHP | 70 | 2025-09-22 | none declared | WooCommerce plugin: catalog/offers import, orders, HTTP protocol |
| [bigperson/exchange1c](https://github.com/bigperson/exchange1c) | PHP | 39 | 2022-07-21 | MIT | HTTP protocol (checkauth/init/file/import/query), catalog import via interfaces |
| [suvit/python-commerceml](https://github.com/suvit/python-commerceml) | Python | 37 | 2014-04-10 | none declared | Django exchange: catalog, offers, orders (abandoned) |
| [gillbeits/commerceml](https://github.com/gillbeits/commerceml) | PHP | 32 | 2021-09-09 | none declared | parser for 2.0x, sample files incl. `rests.xml` |
| [ArtemiusUA/django-cml](https://github.com/ArtemiusUA/django-cml) | Python | 25 | 2019-10-16 | — | Django app: catalog + orders exchange |
| [carono/php-commerceml](https://github.com/carono/php-commerceml) | PHP | 22 | 2023-11-12 | MIT | catalog/offers/orders parser |
| [maxpoletaev/php-commerceml](https://github.com/maxpoletaev/php-commerceml) | PHP | 20 | 2015-08-24 | — | catalog/offers parser |
| [dobromyslov/commerceml-parser](https://github.com/dobromyslov/commerceml-parser) | TypeScript | 15 | 2021-07-07 | MIT | SAX parser for 2.10 (import/offers), "work in progress" |
| [robodal/commerceml-tester](https://github.com/robodal/commerceml-tester) | PHP | 15 | 2018-04-14 | — | random CommerceML file generator + upload tester |
| [kirill-zhirnov/commerceml-js](https://github.com/kirill-zhirnov/commerceml-js) | JavaScript | 6 | 2018-04-10 | — | SAX-style parser |
| [Jurager/commerce](https://github.com/Jurager/commerce) | PHP | 0 | 2026-10-28 | MIT | "universal" CommerceML 2 parser, new |
| [iMega/commerceml2teleport](https://github.com/iMega/commerceml2teleport) | Go | 0 | 2018-06-29 | — | narrow converter |

(Stars and dates as reported by the GitHub API on 2026-11-02.)

## Conclusion

There is no maintained Rust implementation of CommerceML 2 or of the
1C ↔ website exchange protocol. Work proceeds.

What existing projects do not combine, and what this project aims to provide:

- constant-memory streaming over multi-hundred-megabyte exports, not only
  whole-document loading;
- tolerance for real exports that go beyond the official XSD (unknown elements
  are kept as raw XML and reported as warnings, never a hard failure);
- both reading and writing (orders for `type=sale`, round-trip safe);
- the HTTP exchange protocol as a reusable server component with pluggable
  storage and business logic, not tied to a CMS;
- a CLI for validation, statistics, conversion and diffing of exports.

Other-language projects were read only to learn the quirks of real exports.
No code was copied.
