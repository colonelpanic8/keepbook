# Contracts

Shared JSON contract fixtures that must match both the Rust and TypeScript implementations.

These fixtures are intentionally small and stable. When an output format changes, update the
fixture(s) and ensure both test suites pass:

- Rust: `cargo test`
- TypeScript: `cd ts && yarn test`

## Adding A Contract

1. Add a case to `contracts/cases.json` (including any seed state and the CLI command args)
2. Add the expected output JSON to `contracts/<name>.json`

The Rust test `tests/contracts.rs` seeds a temp `data_dir`, runs both CLIs against it, and asserts
that both outputs match the expected JSON.
