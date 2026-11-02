# Security policy

## Supported versions

Until 1.0, only the latest released minor version receives fixes.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private vulnerability
reporting: **Security → Report a vulnerability** on the repository page.

Include what you found, how to reproduce it (a synthetic input file is
ideal) and the impact you expect. You should get a first reply within a
week.

## Scope

Of particular interest:

- crashes, hangs or unbounded memory use in the parser on malicious input
  (the parser is meant to handle untrusted files);
- in `commerceml-exchange`: authentication bypass, session fixation, path
  traversal through `filename`, zip-slip and zip bombs, and exhausting disk
  or memory through uploads.

Denial of service through files that are simply very large but valid is
expected behaviour; use the size limits in the exchange server
configuration.
