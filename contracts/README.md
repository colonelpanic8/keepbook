# Contracts

Shared JSON contract fixtures for stable Rust CLI output.

These fixtures are intentionally small and stable. When an output format changes, update the
fixture(s) and ensure the Rust tests pass:

- `cargo test`

## Adding A Contract

1. Add a case to `contracts/cases.json` (including any seed state and the CLI command args)
2. Add the expected output JSON to `contracts/<name>.json`

The Rust test `tests/contracts.rs` seeds a temp `data_dir`, runs the CLI against it, and asserts
that the output matches the expected JSON.
